//! # Modulo `player` — el jugador
//!
//! De momento es una capa muy fina: una fisica vertical sencilla (gravedad +
//! deteccion de suelo) que se aplica a la camara. En v0.4.x crecera para
//! incluir interaccion (romper/colocar), inventario, salud...

mod controller;

pub use controller::{
    EYE_HEIGHT, PLAYER_HEIGHT, PLAYER_RADIUS, PlayerController, block_overlaps_player,
};
