//! **Punto de clima** del selector multi-parametrico de biomas (seccion 3.2).
//!
//! Es el vector de parametros que decide el bioma. Todos los campos estan
//! normalizados a `[0, 1]` para que la distancia ponderada entre nodos sea
//! comparable. El bioma se elige de este punto **entero**, no de tres valores
//! sueltos: asi la continentalidad y la erosion influyen en la eleccion, que es
//! justo lo que hoy no ocurre (`biomes::select` solo mira temperatura, humedad y
//! elevacion).

/// Vector de parametros climaticos y geograficos de una columna.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClimatePoint {
    /// 0 = oceano profundo, 0.5 = costa, 1 = interior continental.
    pub continentalness: f32,
    /// 0 = montana joven (poco erosionada), 1 = llanura muy erosionada.
    pub erosion: f32,
    /// Relieve local: 0 = valle, 1 = cresta.
    pub peaks: f32,
    /// Temperatura efectiva tras latitud y lapse de altitud.
    pub temperature: f32,
    /// Humedad efectiva.
    pub humidity: f32,
}

impl ClimatePoint {
    /// Vector en el mismo orden que `BiomeNode::target`/`weights`
    /// (`temperature, humidity, continentalness, erosion, peaks`).
    #[inline]
    pub fn to_array(self) -> [f32; 5] {
        [
            self.temperature,
            self.humidity,
            self.continentalness,
            self.erosion,
            self.peaks,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_vector_respeta_el_orden_documentado() {
        let p = ClimatePoint {
            continentalness: 0.1,
            erosion: 0.2,
            peaks: 0.3,
            temperature: 0.4,
            humidity: 0.5,
        };
        assert_eq!(p.to_array(), [0.4, 0.5, 0.1, 0.2, 0.3]);
    }
}
