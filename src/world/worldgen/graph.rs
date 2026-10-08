//! **Grafo de densidad** (DAG) data-driven para el worldgen (Parte C, C1).
//!
//! En vez de una cascada de `if` y ruidos cableados, el terreno se define con un
//! **grafo de nodos**: constantes, ruido fractal, operaciones (add/mul/min/max),
//! splines, gradiente en Y y *domain warp*. Un nodo puede reutilizarse (es un
//! **DAG**, no un arbol) y se evalua en un **orden topologico plano** (sin
//! recursion por voxel ni `dyn`).
//!
//! Este modulo es **puro y determinista**: sin RNG con estado (el ruido sale de
//! un hash de `(salt, x, y, z)`) y sin GPU. Es la base de C2 (evaluacion en
//! retícula + paralelizacion) y C3 (coexistencia con el generador actual).
//!
//! Estado (honesto): C1 implementa el modelo de datos, el validador, el
//! compilador y el evaluador. La integracion en `terrain.rs` (C2), el formato
//! en disco (RON/JSON), `GeneratorKind` y el clima (C4) quedan como pendientes.

use serde::{Deserialize, Serialize};

/// Identificador de nodo dentro de un [`Graph`] (indice en su arena).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u32);

/// Tipo de ruido fractal base.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum NoiseKind {
    /// Ruido 2D (ignora `y`); ideal para continentes/clima (una vez por columna).
    Value2D,
    /// Ruido 3D (cuevas, densidad).
    Value3D,
}

/// Un nodo del grafo.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Node {
    /// Constante.
    Const(f32),
    /// Ruido fractal determinista por `salt`.
    Noise {
        kind: NoiseKind,
        salt: u64,
        frequency: f32,
        octaves: u32,
        persistence: f32,
        lacunarity: f32,
    },
    /// Suma / producto / minimo / maximo de dos entradas.
    Add(NodeId, NodeId),
    Mul(NodeId, NodeId),
    Min(NodeId, NodeId),
    Max(NodeId, NodeId),
    /// Recorta `input` a `[lo, hi]`.
    Clamp {
        input: NodeId,
        lo: f32,
        hi: f32,
    },
    /// Valor absoluto.
    Abs(NodeId),
    /// Curva `input -> valor` (puntos ordenados por x).
    Spline {
        input: NodeId,
        points: Vec<(f32, f32)>,
    },
    /// Gradiente lineal segun `y` (de `from_v` en `from_y` a `to_v` en `to_y`).
    YGradient {
        from_y: i32,
        to_y: i32,
        from_v: f32,
        to_v: f32,
    },
    /// *Domain warp*: evalua `input` desplazado por `dx`/`dz` (en bloques).
    Warp {
        input: NodeId,
        dx: NodeId,
        dz: NodeId,
    },
    /// Marca una parte 2D (una vez por `(x, z)`). El evaluador por-voxel la trata
    /// como su entrada; el evaluador por-columna de C2 la cachea.
    Cache2D(NodeId),
}

/// Errores de validacion/compilacion del grafo.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum GraphError {
    /// Una referencia apunta fuera de la arena.
    BadRef(NodeId),
    /// El grafo tiene un ciclo (no es un DAG).
    Cycle,
    /// Un nodo tiene un valor no finito (NaN/infinito) o parametros invalidos.
    NonFinite(NodeId),
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::BadRef(NodeId(i)) => write!(f, "referencia a nodo inexistente: {i}"),
            GraphError::Cycle => write!(f, "el grafo tiene un ciclo"),
            GraphError::NonFinite(NodeId(i)) => write!(f, "nodo {i} con valor no finito"),
        }
    }
}

impl std::error::Error for GraphError {}

/// Arena de nodos. Los ids son indices; se anaden con [`Graph::push`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Graph {
    nodes: Vec<Node>,
}

impl Graph {
    /// Grafo vacio.
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// Numero de nodos.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// ¿Esta vacio?
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Acceso al nodo (para tests/introspeccion).
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.0 as usize)
    }

    /// Anade un nodo y devuelve su id.
    pub fn push(&mut self, node: Node) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(node);
        id
    }

    /// Serializa el grafo a **JSON** (para `assets/worldgen/*.json` o debug).
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    /// Carga un grafo desde **JSON**.
    pub fn from_json(s: &str) -> Result<Graph, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }

    /// Entradas de un nodo (sus dependencias directas).
    fn inputs(node: &Node) -> Vec<NodeId> {
        match node {
            Node::Const(_) | Node::Noise { .. } | Node::YGradient { .. } => Vec::new(),
            Node::Add(a, b) | Node::Mul(a, b) | Node::Min(a, b) | Node::Max(a, b) => vec![*a, *b],
            Node::Clamp { input, .. }
            | Node::Abs(input)
            | Node::Spline { input, .. }
            | Node::Cache2D(input) => vec![*input],
            Node::Warp { input, dx, dz } => vec![*input, *dx, *dz],
        }
    }

    /// Valida: referencias dentro de la arena, sin ciclos y valores finitos.
    pub fn validate(&self) -> Result<(), GraphError> {
        let n = self.nodes.len();
        for (i, node) in self.nodes.iter().enumerate() {
            Self::check_values(node, NodeId(i as u32))?;
            for input in Self::inputs(node) {
                if input.0 as usize >= n {
                    return Err(GraphError::BadRef(input));
                }
            }
        }
        // Orden topologico: si hay ciclo, falla.
        self.compile().map(|_| ())
    }

    fn check_values(node: &Node, id: NodeId) -> Result<(), GraphError> {
        let bad = |b: bool| {
            if b {
                Err(GraphError::NonFinite(id))
            } else {
                Ok(())
            }
        };
        match node {
            Node::Const(v) => bad(!v.is_finite()),
            Node::Noise {
                frequency,
                persistence,
                lacunarity,
                ..
            } => bad(!frequency.is_finite()
                || !persistence.is_finite()
                || !lacunarity.is_finite()
                || *frequency <= 0.0
                || *lacunarity <= 0.0),
            Node::Clamp { lo, hi, .. } => bad(!lo.is_finite() || !hi.is_finite() || lo > hi),
            Node::Spline { points, .. } => {
                bad(points.len() < 2
                    || points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()))
            }
            Node::YGradient { from_v, to_v, .. } => bad(!from_v.is_finite() || !to_v.is_finite()),
            _ => Ok(()),
        }
    }

    /// Orden topologico plano (todas las entradas antes que su consumidor).
    pub fn compile(&self) -> Result<Program, GraphError> {
        // DFS con marca 0=no visitado, 1=en pila, 2=hecho.
        let n = self.nodes.len();
        let mut state = vec![0u8; n];
        let mut order: Vec<u32> = Vec::with_capacity(n);
        // Pila explicita para no reventar con grafos profundos.
        for start in 0..n {
            if state[start] == 2 {
                continue;
            }
            let mut stack = vec![(start, 0usize)];
            while let Some((i, next)) = stack.pop() {
                let node = &self.nodes[i];
                let ins = Self::inputs(node);
                if next < ins.len() {
                    let child = ins[next].0 as usize;
                    if child >= n {
                        return Err(GraphError::BadRef(ins[next]));
                    }
                    stack.push((i, next + 1));
                    match state[child] {
                        0 => {
                            state[child] = 1;
                            stack.push((child, 0));
                        }
                        1 => return Err(GraphError::Cycle),
                        _ => {}
                    }
                } else {
                    state[i] = 2;
                    order.push(i as u32);
                }
            }
        }
        Ok(Program { order })
    }
}

/// Grafo compilado: solo el orden de evaluacion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    order: Vec<u32>,
}

impl Program {
    /// Orden de evaluacion (ids de nodo).
    pub fn order(&self) -> &[u32] {
        &self.order
    }

    /// Evalua el grafo en un punto y devuelve la **raiz** (el ultimo nodo
    /// anadido, convencion de este modulo). Reserva scratch; para bucles usa
    /// [`Program::eval_into`].
    pub fn eval(&self, graph: &Graph, x: f32, y: f32, z: f32) -> f32 {
        if graph.is_empty() {
            return 0.0;
        }
        let mut buf = vec![0.0f32; graph.nodes.len()];
        self.eval_into(graph, x, y, z, &mut buf);
        buf[graph.nodes.len() - 1]
    }

    /// Evalua reutilizando `buf` (debe medir `graph.len()`).
    pub fn eval_into(&self, graph: &Graph, x: f32, y: f32, z: f32, buf: &mut [f32]) {
        for &i in &self.order {
            let v = match &graph.nodes[i as usize] {
                Node::Const(c) => *c,
                Node::Noise {
                    kind,
                    salt,
                    frequency,
                    octaves,
                    persistence,
                    lacunarity,
                } => fbm(
                    *salt,
                    x * frequency,
                    y * frequency,
                    z * frequency,
                    *octaves,
                    *persistence,
                    *lacunarity,
                    *kind,
                ),
                Node::Add(a, b) => buf[a.0 as usize] + buf[b.0 as usize],
                Node::Mul(a, b) => buf[a.0 as usize] * buf[b.0 as usize],
                Node::Min(a, b) => buf[a.0 as usize].min(buf[b.0 as usize]),
                Node::Max(a, b) => buf[a.0 as usize].max(buf[b.0 as usize]),
                Node::Clamp { input, lo, hi } => buf[input.0 as usize].clamp(*lo, *hi),
                Node::Abs(input) => buf[input.0 as usize].abs(),
                Node::Spline { input, points } => {
                    crate::world::worldgen::math::spline(points, buf[input.0 as usize])
                }
                Node::YGradient {
                    from_y,
                    to_y,
                    from_v,
                    to_v,
                } => {
                    let span = (*to_y - *from_y) as f32;
                    let t = if span.abs() < 1e-6 {
                        0.0
                    } else {
                        ((y - *from_y as f32) / span).clamp(0.0, 1.0)
                    };
                    crate::world::worldgen::math::lerp(*from_v, *to_v, t)
                }
                Node::Warp { input, dx, dz } => {
                    let ox = buf[dx.0 as usize];
                    let oz = buf[dz.0 as usize];
                    eval_node(graph, *input, x + ox, y, z + oz)
                }
                // El evaluador por-voxel no cachea: trata la parte 2D como su
                // entrada. Un evaluador por-columna (C2) la calcula una vez.
                Node::Cache2D(input) => buf[input.0 as usize],
            };
            buf[i as usize] = v;
        }
    }
}

/// Evalua un subgrafo por su id, recursivamente (lo usa `Warp`, que necesita el
/// valor de su entrada en un punto distinto). La recursion es la profundidad del
/// DAG; el camino rapido del programa compilado no la usa.
fn eval_node(graph: &Graph, id: NodeId, x: f32, y: f32, z: f32) -> f32 {
    let Some(node) = graph.node(id) else {
        return 0.0;
    };
    match node {
        Node::Const(c) => *c,
        Node::Noise {
            kind,
            salt,
            frequency,
            octaves,
            persistence,
            lacunarity,
        } => fbm(
            *salt,
            x * frequency,
            y * frequency,
            z * frequency,
            *octaves,
            *persistence,
            *lacunarity,
            *kind,
        ),
        Node::Add(a, b) => eval_node(graph, *a, x, y, z) + eval_node(graph, *b, x, y, z),
        Node::Mul(a, b) => eval_node(graph, *a, x, y, z) * eval_node(graph, *b, x, y, z),
        Node::Min(a, b) => eval_node(graph, *a, x, y, z).min(eval_node(graph, *b, x, y, z)),
        Node::Max(a, b) => eval_node(graph, *a, x, y, z).max(eval_node(graph, *b, x, y, z)),
        Node::Clamp { input, lo, hi } => eval_node(graph, *input, x, y, z).clamp(*lo, *hi),
        Node::Abs(input) => eval_node(graph, *input, x, y, z).abs(),
        Node::Spline { input, points } => {
            crate::world::worldgen::math::spline(points, eval_node(graph, *input, x, y, z))
        }
        Node::YGradient {
            from_y,
            to_y,
            from_v,
            to_v,
        } => {
            let span = (*to_y - *from_y) as f32;
            let t = if span.abs() < 1e-6 {
                0.0
            } else {
                ((y - *from_y as f32) / span).clamp(0.0, 1.0)
            };
            crate::world::worldgen::math::lerp(*from_v, *to_v, t)
        }
        Node::Warp { input, dx, dz } => {
            let ox = eval_node(graph, *dx, x, y, z);
            let oz = eval_node(graph, *dz, x, y, z);
            eval_node(graph, *input, x + ox, y, z + oz)
        }
        Node::Cache2D(input) => eval_node(graph, *input, x, y, z),
    }
}

/// Hash determinista de enteros -> `[0, 1)`.
fn hash01(salt: u64, ix: i32, iy: i32, iz: i32) -> f32 {
    let mut h = salt;
    h ^= (ix as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= (iy as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= (iz as i64 as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    ((h >> 11) as f32) / ((1u64 << 53) as f32)
}

/// Ruido de valor trilineal con interpolacion suave.
fn value_noise(salt: u64, x: f32, y: f32, z: f32) -> f32 {
    let xi = x.floor();
    let yi = y.floor();
    let zi = z.floor();
    let (fx, fy, fz) = (x - xi, y - yi, z - zi);
    let (ux, uy, uz) = (fade(fx), fade(fy), fade(fz));
    let (ix, iy, iz) = (xi as i32, yi as i32, zi as i32);
    let c = |dx, dy, dz| hash01(salt, ix + dx, iy + dy, iz + dz);
    let x00 = c(0, 0, 0) + (c(1, 0, 0) - c(0, 0, 0)) * ux;
    let x10 = c(0, 1, 0) + (c(1, 1, 0) - c(0, 1, 0)) * ux;
    let x01 = c(0, 0, 1) + (c(1, 0, 1) - c(0, 0, 1)) * ux;
    let x11 = c(0, 1, 1) + (c(1, 1, 1) - c(0, 1, 1)) * ux;
    let y0 = x00 + (x10 - x00) * uy;
    let y1 = x01 + (x11 - x01) * uy;
    y0 + (y1 - y0) * uz
}

#[inline]
fn fade(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Ruido fractal (fBm) en `[-1, 1]`.
#[allow(clippy::too_many_arguments)]
fn fbm(
    salt: u64,
    x: f32,
    y: f32,
    z: f32,
    octaves: u32,
    persistence: f32,
    lacunarity: f32,
    kind: NoiseKind,
) -> f32 {
    let octaves = octaves.max(1);
    let mut freq = 1.0f32;
    let mut amp = 1.0f32;
    let mut sum = 0.0f32;
    let mut norm = 0.0f32;
    for o in 0..octaves {
        // 2D: el ruido varia en (x, z) ignorando y. 3D: en (x, y, z).
        let (gy, gz) = match kind {
            NoiseKind::Value2D => (0.0, z),
            NoiseKind::Value3D => (y, z),
        };
        let n = value_noise(
            salt.wrapping_add(o as u64 * 0x9E37_79B9),
            x * freq,
            gy * freq,
            gz * freq,
        );
        sum += n * amp;
        norm += amp;
        freq *= lacunarity;
        amp *= persistence;
    }
    // n en [0,1) -> fBm en [-1, 1).
    (sum / norm) * 2.0 - 1.0
}

/// Grafo por defecto que produce la **altura** del terreno (8..200): una spline
/// de continentalidad (ruido 2D de baja frecuencia) mas detalle de alta
/// frecuencia, sumado a un nivel base. Determinista por `seed`.
pub fn default_height_graph(seed: u64) -> Graph {
    let mut g = Graph::new();
    let cont = g.push(Node::Noise {
        kind: NoiseKind::Value2D,
        salt: seed,
        frequency: 0.0022,
        octaves: 4,
        persistence: 0.5,
        lacunarity: 2.0,
    });
    let spline = g.push(Node::Spline {
        input: cont,
        points: vec![
            (-1.0, -26.0),
            (-0.3, -8.0),
            (0.0, 2.0),
            (0.35, 18.0),
            (1.0, 46.0),
        ],
    });
    let detail = g.push(Node::Noise {
        kind: NoiseKind::Value2D,
        salt: seed ^ 0x9E37_79B9_7F4A_7C15,
        frequency: 0.03,
        octaves: 3,
        persistence: 0.5,
        lacunarity: 2.0,
    });
    let detail_amp = g.push(Node::Const(9.0));
    let detail_scaled = g.push(Node::Mul(detail, detail_amp));
    let relief = g.push(Node::Add(spline, detail_scaled));
    let base = g.push(Node::Const(64.0));
    let height = g.push(Node::Add(relief, base));
    // Raiz (ultimo nodo): altura recortada al rango del mundo.
    g.push(Node::Clamp {
        input: height,
        lo: 8.0,
        hi: 200.0,
    });
    g
}

/// Grafo por defecto que produce un campo de **densidad 3D**: `superficie(x,z) - y
/// + cueva`. Donde la densidad es `> 0` hay solido; el ruido 3D (recortado a la
/// parte positiva y restado) cava cuevas/tuneles sin crear islas flotantes. Es la
/// base del camino `Graph` (estilo 1.18).
pub fn default_density_graph(seed: u64) -> Graph {
    let mut g = Graph::new();
    // Superficie por (x, z): continentalidad + detalle.
    let cont = g.push(Node::Noise {
        kind: NoiseKind::Value2D,
        salt: seed,
        frequency: 0.0022,
        octaves: 4,
        persistence: 0.5,
        lacunarity: 2.0,
    });
    let spline = g.push(Node::Spline {
        input: cont,
        points: vec![
            (-1.0, -26.0),
            (-0.3, -8.0),
            (0.0, 2.0),
            (0.35, 18.0),
            (1.0, 46.0),
        ],
    });
    let detail = g.push(Node::Noise {
        kind: NoiseKind::Value2D,
        salt: seed ^ 0x9E37_79B9_7F4A_7C15,
        frequency: 0.03,
        octaves: 3,
        persistence: 0.5,
        lacunarity: 2.0,
    });
    let detail_amp = g.push(Node::Const(9.0));
    let detail_scaled = g.push(Node::Mul(detail, detail_amp));
    let relief = g.push(Node::Add(spline, detail_scaled));
    let base = g.push(Node::Const(64.0));
    let surface = g.push(Node::Add(relief, base));
    // `-y`: gradiente de 0 (y=0) a -WORLD_HEIGHT (y=384).
    let y_sub = g.push(Node::YGradient {
        from_y: 0,
        to_y: 384,
        from_v: 0.0,
        to_v: -384.0,
    });
    // Ruido 3D que cava tuneles: se recorta a la parte positiva (solo cava donde
    // supera el umbral) y se resta; asi no baja toda la superficie ni flota.
    let cave = g.push(Node::Noise {
        kind: NoiseKind::Value3D,
        salt: seed ^ 0xD1B5_4A32_D192_ED03,
        frequency: 0.07,
        octaves: 3,
        persistence: 0.5,
        lacunarity: 2.0,
    });
    let threshold = g.push(Node::Const(-0.15));
    let shifted = g.push(Node::Add(cave, threshold));
    let zero = g.push(Node::Const(0.0));
    let carve01 = g.push(Node::Max(shifted, zero));
    let carve_amp = g.push(Node::Const(-70.0));
    let carve = g.push(Node::Mul(carve01, carve_amp));
    // densidad = superficie - y + carve  (raiz = ultimo nodo).
    let d1 = g.push(Node::Add(surface, y_sub));
    g.push(Node::Add(d1, carve));
    g
}

/// Grafo de **clima**: una banda de clima en `0..1` (temperatura o lluvia) segun
/// el `salt`. Ruido 2D de baja frecuencia con contraste para que haya regiones
/// claramente calidas/frias o secas/humedas.
pub fn climate_graph(seed: u64, salt: u64) -> Graph {
    let mut g = Graph::new();
    let n = g.push(Node::Noise {
        kind: NoiseKind::Value2D,
        salt: seed ^ salt,
        frequency: 0.0015,
        octaves: 4,
        persistence: 0.5,
        lacunarity: 2.0,
    });
    let half = g.push(Node::Const(0.5));
    let n_half = g.push(Node::Mul(n, half));
    let band = g.push(Node::Add(n_half, half)); // 0..1
    // Contraste: aleja de 0.5 para repartir mejor los biomas.
    let neg_half = g.push(Node::Const(-0.5));
    let centered = g.push(Node::Add(band, neg_half));
    let gain = g.push(Node::Const(1.6));
    let stretched = g.push(Node::Mul(centered, gain));
    let recentered = g.push(Node::Add(stretched, half));
    g.push(Node::Clamp {
        input: recentered,
        lo: 0.0,
        hi: 1.0,
    });
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis_plane() -> Graph {
        // g = clamp(spline(noise2d) * 40 + 64, 0, 200): un terreno de juguete.
        let mut g = Graph::new();
        let n = g.push(Node::Noise {
            kind: NoiseKind::Value2D,
            salt: 7,
            frequency: 0.01,
            octaves: 4,
            persistence: 0.5,
            lacunarity: 2.0,
        });
        let s = g.push(Node::Spline {
            input: n,
            points: vec![(-1.0, 0.0), (0.0, 0.5), (1.0, 1.0)],
        });
        let k = g.push(Node::Const(40.0));
        let m = g.push(Node::Mul(s, k));
        let base = g.push(Node::Const(64.0));
        let a = g.push(Node::Add(m, base));
        let c = g.push(Node::Clamp {
            input: a,
            lo: 0.0,
            hi: 200.0,
        });
        // El ultimo nodo es la raiz.
        assert_eq!(c, NodeId(6));
        g
    }

    #[test]
    fn valida_y_compila_un_grafo_simple() {
        let g = axis_plane();
        g.validate().unwrap();
        let prog = g.compile().unwrap();
        // El orden respeta las dependencias: el ruido (0) antes que la spline (1).
        let pos = |id: u32| prog.order().iter().position(|&x| x == id).unwrap();
        assert!(pos(0) < pos(1));
        assert!(pos(1) < pos(3));
    }

    #[test]
    fn detecta_referencias_invalidas_y_ciclos() {
        let mut bad = Graph::new();
        bad.push(Node::Add(NodeId(0), NodeId(5))); // 5 no existe
        assert_eq!(bad.validate(), Err(GraphError::BadRef(NodeId(5))));

        let mut cyc = Graph::new();
        let a = cyc.push(Node::Const(1.0));
        let b = cyc.push(Node::Add(a, a));
        // a = Add(b, b): forzamos el ciclo reemplazando el nodo 0.
        cyc.nodes[0] = Node::Add(b, b);
        assert_eq!(cyc.validate(), Err(GraphError::Cycle));
    }

    #[test]
    fn rechaza_valores_no_finitos() {
        let mut g = Graph::new();
        g.push(Node::Const(f32::NAN));
        assert!(matches!(g.validate(), Err(GraphError::NonFinite(_))));

        let mut g2 = Graph::new();
        g2.push(Node::Noise {
            kind: NoiseKind::Value2D,
            salt: 1,
            frequency: 0.0, // invalida
            octaves: 1,
            persistence: 0.5,
            lacunarity: 2.0,
        });
        assert!(matches!(g2.validate(), Err(GraphError::NonFinite(_))));
    }

    #[test]
    fn evalua_operaciones_basicas() {
        let mut g = Graph::new();
        let a = g.push(Node::Const(3.0));
        let b = g.push(Node::Const(4.0));
        let sum = g.push(Node::Add(a, b));
        let prod = g.push(Node::Mul(a, b));
        let cl = g.push(Node::Clamp {
            input: prod,
            lo: 0.0,
            hi: 10.0,
        });
        let prog = g.compile().unwrap();
        assert!((prog.eval(&g, 0.0, 0.0, 0.0) - 10.0).abs() < 1e-5); // clamp(12) = 10
        let _ = (sum, cl);
    }

    #[test]
    fn el_terreno_de_juguete_es_determinista_y_acotado() {
        let g = axis_plane();
        let prog = g.compile().unwrap();
        let a = prog.eval(&g, 12.0, 0.0, 34.0);
        let b = prog.eval(&g, 12.0, 0.0, 34.0);
        assert_eq!(a, b, "el ruido debe ser determinista");
        for x in -50..50 {
            let h = prog.eval(&g, x as f32 * 7.0, 0.0, 0.0);
            assert!((0.0..=200.0).contains(&h), "altura fuera de rango: {h}");
        }
    }

    #[test]
    fn el_ruido_cambia_con_la_semilla_y_la_posicion() {
        let mut g = Graph::new();
        let n1 = g.push(Node::Noise {
            kind: NoiseKind::Value3D,
            salt: 1,
            frequency: 0.1,
            octaves: 3,
            persistence: 0.5,
            lacunarity: 2.0,
        });
        let n2 = g.push(Node::Noise {
            kind: NoiseKind::Value3D,
            salt: 2,
            frequency: 0.1,
            octaves: 3,
            persistence: 0.5,
            lacunarity: 2.0,
        });
        let prog = g.compile().unwrap();
        let a = prog.eval(&g, 1.3, 2.7, 3.1);
        let b = prog.eval(&g, 5.3, 2.7, 3.1);
        assert!((a - b).abs() > 1e-4, "el ruido no varia con la posicion");
        let mut buf = vec![0.0; g.len()];
        prog.eval_into(&g, 1.3, 2.7, 3.1, &mut buf);
        assert!((buf[n1.0 as usize] - buf[n2.0 as usize]).abs() > 1e-6);
    }

    #[test]
    fn el_grafo_por_defecto_da_alturas_validas() {
        let g = default_height_graph(13_371);
        g.validate().unwrap();
        let prog = g.compile().unwrap();
        let (mut min, mut max) = (f32::INFINITY, f32::NEG_INFINITY);
        for i in 0..300 {
            let x = i as f32 * 13.0;
            let z = (i as f32 * 7.0) % 300.0;
            let h = prog.eval(&g, x, 0.0, z);
            assert!((8.0..=200.0).contains(&h), "altura fuera de rango: {h}");
            min = min.min(h);
            max = max.max(h);
        }
        assert!(max - min > 5.0, "el grafo deberia variar la altura");
    }

    #[test]
    fn el_grafo_va_y_vuelve_por_json() {
        let g = default_height_graph(7);
        let json = g.to_json().unwrap();
        let back = Graph::from_json(&json).unwrap();
        assert_eq!(back.len(), g.len());
        let p1 = g.compile().unwrap();
        let p2 = back.compile().unwrap();
        for x in [0.0, 100.0, -50.0] {
            assert_eq!(p1.eval(&g, x, 0.0, x), p2.eval(&back, x, 0.0, x));
        }
    }
}
