# Decisiones de diseño (DECISIONS.md)

Registro de decisiones tecnicas de Solaria Voxel. Cada entrada explica **que**
se decidio, **por que** y **que alternativas** se descartaron. Se anade una
entrada por cada decision relevante, no por cada linea de codigo.

Formato: `## [fecha] vX.Y.Z — Titulo`

---

## v0.1.0 — Base del proyecto

### 2026-10-04 — El motor se escribe desde cero, sin motor de juego

**Decision.** No usamos Bevy, ni Fyrox, ni Godot. Construimos el motor nosotros.

**Motivo.** Es un proyecto de aprendizaje: queremos entender y controlar cada
pieza (bucle de juego, matematica, GPU, meshing, versionado de mundo). Un motor
completo esconde justo lo que queremos aprender.

**Consecuencia.** Asumimos mas trabajo y mas codigo propio. A cambio, cada linea
esta documentada y bajo nuestro control.

### 2026-10-04 — Capa de plataforma: `winit` + `wgpu` (no OpenGL crudo)

**Decision.** Para ventana y GPU usamos `winit` (ventana/eventos) y `wgpu`
(abstraccion de GPU). No escribimos Vulkan/D3D12 directamente.

**Motivo.**
- Escribir el binding nativo de Vulkan/D3D12/Metal desde cero seria meses de
  trabajo ajeno al objetivo (el motor de voxeles), no mas aprendizaje del motor.
- `wgpu` es *una capa de bajo nivel*, no un motor: nos da buffers, pipelines y
  shaders, y nos obliga a escribir el renderizado a mano. Sigue siendo
  "desde cero" a nivel de motor.
- Multiplataforma (Vulkan en Windows/Linux, Metal en macOS) con un solo codigo.

**Alternativas descartadas.** `softbuffer` + rasterizador propio (demasiado
lento y no representa lo que hara un motor real); OpenGL crudo con `glow`
(API antigua, peor encaje futuro con WebGPU).

### 2026-10-04 — Separacion por modulos `engine` / `render` / `scene` / `math`

**Decision.** El codigo se organiza en cuatro modulos con responsabilidades
claras, y el renderer es una **envoltura fina** sobre wgpu.

**Motivo.** Que la logica del juego no sepa de wgpu. Si algun dia cambiamos de
API grafica, solo cambia `render`. Facilita testear `math` y `scene` sin GPU.

### 2026-10-04 — Matematica propia en lugar de `glam`

**Decision.** Implementamos `Vec3` y `Mat4` nosotros, en `src/math`.

**Motivo.** Entender de verdad matrices, proyeccion y camara. Es la base que
usaremos para transformar chunks y para el frustum culling (v0.11.2).

**Nota.** Si el rendimiento de nuestra matematica se vuelve un problema, se
puede sustituir por `glam` mas adelante sin tocar el resto del motor: los
modulos solo dependen de la interfaz de `math`.

### 2026-10-04 — Convenio de ejes: mano derecha, -Z al frente, column-major

**Decision.**
- Sistema de mano derecha: +X derecha, +Y arriba, **-Z al frente**.
- Matrices en **column-major**, listas para subir a la GPU sin transponer.

**Motivo.** Encaja con Minecraft, con la convencion de wgpu (profundidad en
`[0,1]`) y con `raw-window-handle`. Documentado en el modulo `math` para no
volver a decidirlo.

### 2026-10-04 — Color de cielo definido en sRGB y convertido a lineal

**Decision.** Elegimos el azul cielo en espacio sRGB (como se ve en pantalla) y
lo convertimos a espacio lineal antes de pasarlo a `LoadOp::Clear`.

**Motivo.** Preferimos un formato de superficie sRGB (lo forzamos si esta
disponible). En ese caso la GPU espera colores lineales y aplica ella la
correccion de gamma; pasar valores sRGB directos daria un color mas claro del
intencionado.

### 2026-10-04 — Perfiles de compilacion: dependencias optimizadas en `dev`

**Decision.** En `[profile.dev]` ponemos `opt-level = 1` para nuestro codigo, y
`[profile.dev.package."*"] opt-level = 3` para las dependencias.

**Motivo.** Una build de `wgpu` sin optimizar va a pocos FPS y hace que el
desarrollo se sienta roto. Optimizar solo las dependencias da fluidez sin
sacrificar tiempos de compilacion de nuestro crate.

---

## Plantilla para futuras entradas

```
### [fecha] vX.Y.Z — Titulo
**Decision.** ...
**Motivo.** ...
**Alternativas descartadas.** ...
**Consecuencia.** ...
```
