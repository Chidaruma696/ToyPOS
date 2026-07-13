//! `domain` — núcleo de ToyPOS: entidades y reglas **puras**, sin I/O.
//!
//! Toda regla de negocio (acceso, precios, aprobaciones, corte) vive aquí una
//! sola vez (DRY, D1). Las tres caras futuras (etiquetado, POS, admin) y la capa
//! `storage` cuelgan de este crate sin reimplementar reglas.

pub mod acceso;
pub mod aprobacion;
pub mod barcode;
pub mod caja;
pub mod error;
pub mod folio;
pub mod gasto;
pub mod inventario;
pub mod precio;
pub mod producto;
pub mod sesion;
pub mod sucursal;
pub mod tiempo;
pub mod unidades;
pub mod usuario;

pub use error::ErrorDominio;
