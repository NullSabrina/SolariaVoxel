//! # Modulo `player` — el jugador
//!
//! Fisica del jugador: gravedad, suelo, salto, vuelo y **colision horizontal**
//! (el jugador es una caja que no atraviesa paredes y se desliza por ellas).
//! Mas adelante crecera para incluir inventario, salud...

mod controller;

pub use controller::{
    EYE_HEIGHT, PLAYER_HEIGHT, PLAYER_RADIUS, PlayerController, block_overlaps_player,
};
