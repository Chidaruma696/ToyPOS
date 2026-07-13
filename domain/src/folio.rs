//! Folio de documentos: identificador humano `{código_sucursal}{tipo}{consecutivo}`
//! todo junto, sin separadores (p. ej. `SNJC56`), aparte del `uuid` técnico (D18).
//!
//! El consecutivo es secuencial por `(sucursal, tipo)` y **sin reutilización**:
//! un documento cancelado o revertido conserva su folio. La *secuencia* (el
//! próximo consecutivo) la persiste la capa de almacenamiento; aquí solo vive el
//! **formato** y el marcador de tipo.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::sucursal::CodigoSucursal;

/// Tipo de documento foliable. Cada tipo lleva una letra; sumar un tipo futuro
/// (envío, pedido, devolución) no toca los existentes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TipoDocumento {
    Corte,
    Nota,
    Gasto,
}

impl TipoDocumento {
    #[must_use]
    pub fn letra(self) -> char {
        match self {
            TipoDocumento::Corte => 'C',
            TipoDocumento::Nota => 'N',
            TipoDocumento::Gasto => 'G',
        }
    }
}

/// Folio legible ya compuesto. Se asigna **al confirmarse** el documento.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Folio(String);

impl Folio {
    /// Compone `{código}{letra}{consecutivo}` sin separadores.
    #[must_use]
    pub fn componer(codigo: &CodigoSucursal, tipo: TipoDocumento, consecutivo: u64) -> Self {
        Folio(format!("{}{}{consecutivo}", codigo.get(), tipo.letra()))
    }

    /// Rehidrata un folio ya emitido desde su texto persistido.
    #[must_use]
    pub fn desde_texto(texto: String) -> Self {
        Folio(texto)
    }

    #[must_use]
    pub fn get(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Folio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compone_folio_sin_separadores() {
        let cod = CodigoSucursal::nueva("SNJ").unwrap();
        assert_eq!(
            Folio::componer(&cod, TipoDocumento::Corte, 56).get(),
            "SNJC56"
        );
        assert_eq!(
            Folio::componer(&cod, TipoDocumento::Gasto, 204).get(),
            "SNJG204"
        );
        assert_eq!(
            Folio::componer(&cod, TipoDocumento::Nota, 13280).get(),
            "SNJN13280"
        );
    }
}
