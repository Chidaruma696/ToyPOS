//! Catálogo de productos tipado por **unidad de venta**, con **origen** como eje
//! aparte (D22). El tipo y el origen son inmutables tras el alta.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::barcode::Ean13;
use crate::error::ErrorDominio;
use crate::tiempo::Instante;
use crate::unidades::Gramos;

/// Cómo se cobra el producto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoProducto {
    /// Por kilogramo: monto = precio/kg × peso real.
    PesoVariable,
    /// Por unidad a precio fijo.
    Pieza,
}

impl TipoProducto {
    /// Unidad de venta derivada del tipo (4.2).
    #[must_use]
    pub fn unidad_venta(self) -> UnidadVenta {
        match self {
            TipoProducto::PesoVariable => UnidadVenta::Kilogramo,
            TipoProducto::Pieza => UnidadVenta::Unidad,
        }
    }
}

/// De dónde viene el producto (eje independiente del tipo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrigenProducto {
    /// Producido por matriz (acuña su propio barcode).
    Matriz,
    /// Comprado a proveedor externo (trae su GTIN de fábrica).
    Externo,
}

/// Unidad de venta, **derivada** del tipo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnidadVenta {
    Kilogramo,
    Unidad,
}

/// Código de barras de un producto `pieza`, según su origen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodigoBarras {
    /// GTIN de fábrica (producto comprado).
    Externo(Ean13),
    /// EAN-13 acuñado por matriz (producto producido).
    Matriz(Ean13),
}

impl CodigoBarras {
    #[must_use]
    pub fn ean(&self) -> &Ean13 {
        match self {
            CodigoBarras::Externo(e) | CodigoBarras::Matriz(e) => e,
        }
    }
}

/// Producto del catálogo global. `codigo` solo aplica a `pieza`; `peso_variable`
/// no lleva código de producto (se etiqueta por peso, futuro).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Producto {
    pub id: Uuid,
    pub nombre: String,
    pub tipo: TipoProducto,
    pub origen: OrigenProducto,
    pub codigo: Option<CodigoBarras>,
    /// Peso de empaque informativo (peso fijo); no interviene en el precio.
    pub peso_empaque: Option<Gramos>,
    pub activo: bool,
    pub actualizado: Instante,
}

impl Producto {
    /// Alta con validación de las reglas cruzadas de tipo/origen/código (D22):
    /// - `PesoVariable` ⇒ sin código y sin peso de empaque (su peso es por-ítem).
    /// - `Pieza` ⇒ con código, cuyo origen concuerda con el del producto.
    pub fn nuevo(
        nombre: &str,
        tipo: TipoProducto,
        origen: OrigenProducto,
        codigo: Option<CodigoBarras>,
        peso_empaque: Option<Gramos>,
        ahora: Instante,
    ) -> Result<Self, ErrorDominio> {
        if nombre.trim().is_empty() {
            return Err(ErrorDominio::Invalido(
                "el nombre del producto es obligatorio".into(),
            ));
        }
        if tipo == TipoProducto::PesoVariable && peso_empaque.is_some() {
            return Err(ErrorDominio::Regla(
                "el peso de empaque es solo de productos pieza (peso fijo); \
                 el peso de un peso-variable se captura por ítem al pesar"
                    .into(),
            ));
        }
        match (tipo, &codigo) {
            (TipoProducto::PesoVariable, Some(_)) => {
                return Err(ErrorDominio::Regla(
                    "un producto peso-variable no lleva código de barras de producto".into(),
                ));
            }
            (TipoProducto::Pieza, None) => {
                return Err(ErrorDominio::Regla(
                    "un producto pieza requiere código de barras".into(),
                ));
            }
            (TipoProducto::Pieza, Some(c)) => {
                let concuerda = matches!(
                    (origen, c),
                    (OrigenProducto::Externo, CodigoBarras::Externo(_))
                        | (OrigenProducto::Matriz, CodigoBarras::Matriz(_))
                );
                if !concuerda {
                    return Err(ErrorDominio::Regla(
                        "el origen del código no concuerda con el origen del producto".into(),
                    ));
                }
            }
            (TipoProducto::PesoVariable, None) => {}
        }
        Ok(Producto {
            id: Uuid::new_v4(),
            nombre: nombre.trim().to_string(),
            tipo,
            origen,
            codigo,
            peso_empaque,
            activo: true,
            actualizado: ahora,
        })
    }

    /// Unidad de venta derivada del tipo (4.2).
    #[must_use]
    pub fn unidad_venta(&self) -> UnidadVenta {
        self.tipo.unidad_venta()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ahora() -> Instante {
        Instante::ahora()
    }

    #[test]
    fn peso_variable_sin_codigo_y_unidad_kg() {
        let p = Producto::nuevo(
            "pollo",
            TipoProducto::PesoVariable,
            OrigenProducto::Matriz,
            None,
            None,
            ahora(),
        )
        .unwrap();
        assert_eq!(p.unidad_venta(), UnidadVenta::Kilogramo);
    }

    #[test]
    fn peso_variable_con_codigo_se_rechaza() {
        let ean = Ean13::parse("7501059224827").unwrap();
        let r = Producto::nuevo(
            "pollo",
            TipoProducto::PesoVariable,
            OrigenProducto::Externo,
            Some(CodigoBarras::Externo(ean)),
            None,
            ahora(),
        );
        assert!(r.is_err());
    }

    #[test]
    fn peso_variable_con_peso_empaque_se_rechaza() {
        // El peso de empaque es del preempacado (pieza); en peso-variable el
        // peso se captura por ítem al pesar.
        let r = Producto::nuevo(
            "pollo",
            TipoProducto::PesoVariable,
            OrigenProducto::Matriz,
            None,
            Some(Gramos::new(500)),
            ahora(),
        );
        assert!(r.is_err());
    }

    #[test]
    fn pieza_externa_lleva_gtin() {
        let ean = Ean13::parse("7501059224827").unwrap();
        let p = Producto::nuevo(
            "Nescafé",
            TipoProducto::Pieza,
            OrigenProducto::Externo,
            Some(CodigoBarras::Externo(ean)),
            None,
            ahora(),
        )
        .unwrap();
        assert_eq!(p.unidad_venta(), UnidadVenta::Unidad);
    }

    #[test]
    fn pieza_matriz_peso_fijo_informativo() {
        let ean = Ean13::generar_matriz(crate::barcode::PREFIJO_MATRIZ, 7).unwrap();
        let p = Producto::nuevo(
            "queso 500 g",
            TipoProducto::Pieza,
            OrigenProducto::Matriz,
            Some(CodigoBarras::Matriz(ean)),
            Some(Gramos::new(500)),
            ahora(),
        )
        .unwrap();
        assert_eq!(p.peso_empaque, Some(Gramos::new(500)));
    }

    #[test]
    fn pieza_con_origen_de_codigo_discordante_se_rechaza() {
        let ean = Ean13::parse("7501059224827").unwrap();
        let r = Producto::nuevo(
            "x",
            TipoProducto::Pieza,
            OrigenProducto::Matriz,
            Some(CodigoBarras::Externo(ean)),
            None,
            ahora(),
        );
        assert!(r.is_err());
    }
}
