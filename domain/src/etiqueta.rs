//! Etiquetado: etiquetas **por-ítem** con identidad propia (D31), caducidad
//! nominal congelada al etiquetar (D32/D33) y **cajas** que agrupan lo
//! etiquetado (D35).
//!
//! La caducidad es informativa: **jamás bloquea** operación alguna (D32). La
//! `fecha_etiquetado` se guarda internamente y nunca se imprime. Etiquetar no
//! mueve inventario: matriz produce sin llevar stock propio (D34).

use jiff::Span;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::acceso::{ContextoAcceso, Permiso};
use crate::barcode::Ean13;
use crate::error::ErrorDominio;
use crate::producto::{OrigenProducto, Producto, TipoProducto, VidaUtil};
use crate::tiempo::{Instante, ZonaHoraria};
use crate::unidades::Gramos;

/// Estado de una etiqueta o caja. `Vendida` queda reservada para el POS futuro;
/// hoy solo se construye `Activa` (mismo patrón que los tipos de movimiento).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstadoEtiqueta {
    Activa,
    Vendida,
}

/// Caducidad nominal (D32/D33): `fecha_etiquetado + vida_util` en meses
/// calendáricos, con "el día" definido en la zona de la sucursal (D21). El
/// ajuste de fin de mes lo resuelve la aritmética civil (31/05 + 9 → 28/29-02).
pub fn calcular_caducidad(
    fecha_etiquetado: Instante,
    zona: &ZonaHoraria,
    vida_util: VidaUtil,
) -> Result<Date, ErrorDominio> {
    let dia = fecha_etiquetado.dia_local(zona);
    dia.checked_add(Span::new().months(vida_util.meses()))
        .map_err(|e| ErrorDominio::Invalido(format!("caducidad fuera de rango: {e}")))
}

/// Fecha de caducidad como se imprime: `DD/MM/AAAA` (D32).
#[must_use]
pub fn formatear_caducidad(fecha: Date) -> String {
    format!(
        "{:02}/{:02}/{:04}",
        fecha.day(),
        fecha.month(),
        fecha.year()
    )
}

/// Contenido imprimible de una etiqueta: **exactamente tres elementos** (D32),
/// de arriba hacia abajo. El tipo mismo no puede portar precio (D8) ni fecha de
/// etiquetado: no existen como campos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContenidoEtiqueta {
    pub barcode: String,
    pub nombre: String,
    /// `DD/MM/AAAA`; `None` solo en la caja de un producto externo (D35).
    pub caducidad: Option<String>,
}

impl ContenidoEtiqueta {
    /// El nombre en **hasta dos líneas** del ancho dado; lo que no cabe se
    /// trunca (la etiqueta física es de tamaño fijo).
    #[must_use]
    pub fn lineas_nombre(&self, ancho: usize) -> Vec<String> {
        let mut lineas: Vec<String> = Vec::new();
        for palabra in self.nombre.split_whitespace() {
            let cabe = lineas
                .last()
                .is_some_and(|l| l.len() + 1 + palabra.len() <= ancho);
            if cabe {
                let ultima = lineas.last_mut().expect("cabe implica última línea");
                ultima.push(' ');
                ultima.push_str(palabra);
            } else if lineas.len() < 2 {
                lineas.push(palabra.to_string());
            } else {
                break;
            }
        }
        lineas
    }
}

/// Etiqueta por-ítem de un producto `peso_variable` (D31): identidad propia
/// (discriminador antiduplicado), peso real y caducidad congelada. El barcode
/// carga solo peso + discriminador; el resto vive en este registro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Etiqueta {
    pub id: Uuid,
    pub codigo: Ean13,
    pub discriminador: u64,
    pub producto: Uuid,
    pub peso: Gramos,
    pub sucursal: Uuid,
    /// Caja que la agrupa, si ya se cerró una (D35).
    pub caja: Option<Uuid>,
    /// Interna: **nunca** se imprime (D32).
    pub fecha_etiquetado: Instante,
    pub caducidad: Date,
    pub estado: EstadoEtiqueta,
    pub actualizado: Instante,
}

impl Etiqueta {
    /// Etiqueta una pesada. Exige `etiquetar` + alcance (D28-bis), que el
    /// producto sea `peso_variable` (los `pieza` usan su barcode estable, D35)
    /// y un peso codificable; congela la caducidad (D32).
    pub fn nueva(
        ctx: &ContextoAcceso,
        producto: &Producto,
        sucursal: Uuid,
        zona: &ZonaHoraria,
        peso: Gramos,
        discriminador: u64,
        ahora: Instante,
    ) -> Result<Self, ErrorDominio> {
        ctx.requiere_en(Permiso::Etiquetar, sucursal)?;
        if producto.tipo != TipoProducto::PesoVariable {
            return Err(ErrorDominio::Regla(
                "solo los productos peso-variable llevan etiqueta por-ítem; \
                 un pieza usa su barcode estable y se registra por caja"
                    .into(),
            ));
        }
        let codigo = Ean13::generar_etiqueta(discriminador, peso)?;
        let caducidad = calcular_caducidad(ahora, zona, producto.vida_util)?;
        Ok(Etiqueta {
            id: Uuid::new_v4(),
            codigo,
            discriminador,
            producto: producto.id,
            peso,
            sucursal,
            caja: None,
            fecha_etiquetado: ahora,
            caducidad,
            estado: EstadoEtiqueta::Activa,
            actualizado: ahora,
        })
    }

    /// Contenido imprimible de la etiqueta (D32): barcode, nombre, caducidad.
    #[must_use]
    pub fn contenido(&self, nombre_producto: &str) -> ContenidoEtiqueta {
        ContenidoEtiqueta {
            barcode: self.codigo.get().to_string(),
            nombre: nombre_producto.to_string(),
            caducidad: Some(formatear_caducidad(self.caducidad)),
        }
    }
}

/// Caja que agrupa lo etiquetado (D35): etiquetas por-ítem (`peso_variable`) o
/// una cantidad de unidades idénticas (`pieza`). Es la unidad natural del
/// traslado futuro (envío).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CajaEtiquetado {
    pub id: Uuid,
    pub producto: Uuid,
    pub sucursal: Uuid,
    /// Unidades contenidas; `None` en una caja de pesadas (sus etiquetas la describen).
    pub cantidad: Option<i64>,
    pub fecha_etiquetado: Instante,
    /// `None` en productos externos: conservan la caducidad de su fábrica (D35).
    pub caducidad: Option<Date>,
    pub estado: EstadoEtiqueta,
    pub actualizado: Instante,
}

impl CajaEtiquetado {
    /// Cierra una caja de pesadas: agrupa etiquetas ya emitidas del **mismo
    /// producto y sucursal**, activas y sin caja previa. Su caducidad es la más
    /// próxima de sus etiquetas (rotación FIFO, D32).
    pub fn cerrar_pesadas(
        ctx: &ContextoAcceso,
        producto: &Producto,
        etiquetas: &[Etiqueta],
        ahora: Instante,
    ) -> Result<Self, ErrorDominio> {
        let Some(primera) = etiquetas.first() else {
            return Err(ErrorDominio::Invalido(
                "una caja de pesadas requiere al menos una etiqueta".into(),
            ));
        };
        let sucursal = primera.sucursal;
        ctx.requiere_en(Permiso::Etiquetar, sucursal)?;
        if producto.tipo != TipoProducto::PesoVariable {
            return Err(ErrorDominio::Regla(
                "una caja de pesadas es de un producto peso-variable".into(),
            ));
        }
        for e in etiquetas {
            if e.producto != producto.id || e.sucursal != sucursal {
                return Err(ErrorDominio::Regla(
                    "todas las etiquetas de la caja deben ser del mismo producto y sucursal".into(),
                ));
            }
            if e.estado != EstadoEtiqueta::Activa || e.caja.is_some() {
                return Err(ErrorDominio::Regla(
                    "la caja solo agrupa etiquetas activas que no estén ya en otra caja".into(),
                ));
            }
        }
        let caducidad = etiquetas.iter().map(|e| e.caducidad).min();
        Ok(CajaEtiquetado {
            id: Uuid::new_v4(),
            producto: producto.id,
            sucursal,
            cantidad: None,
            fecha_etiquetado: ahora,
            caducidad,
            estado: EstadoEtiqueta::Activa,
            actualizado: ahora,
        })
    }

    /// Cierra una caja de un producto `pieza`: sin pesar y sin etiquetas
    /// por-ítem, solo la **cantidad contenida** (D35). La caducidad impresa
    /// aplica únicamente a lo producido por matriz.
    pub fn cerrar_pieza(
        ctx: &ContextoAcceso,
        producto: &Producto,
        sucursal: Uuid,
        zona: &ZonaHoraria,
        cantidad: i64,
        ahora: Instante,
    ) -> Result<Self, ErrorDominio> {
        ctx.requiere_en(Permiso::Etiquetar, sucursal)?;
        if producto.tipo != TipoProducto::Pieza {
            return Err(ErrorDominio::Regla(
                "una caja por cantidad es de un producto pieza; \
                 el peso-variable se etiqueta por pesada"
                    .into(),
            ));
        }
        if cantidad <= 0 {
            return Err(ErrorDominio::Invalido(format!(
                "la cantidad de la caja debe ser positiva: {cantidad}"
            )));
        }
        let caducidad = match producto.origen {
            OrigenProducto::Matriz => Some(calcular_caducidad(ahora, zona, producto.vida_util)?),
            OrigenProducto::Externo => None,
        };
        Ok(CajaEtiquetado {
            id: Uuid::new_v4(),
            producto: producto.id,
            sucursal,
            cantidad: Some(cantidad),
            fecha_etiquetado: ahora,
            caducidad,
            estado: EstadoEtiqueta::Activa,
            actualizado: ahora,
        })
    }

    /// Contenido imprimible de la **etiqueta de caja** de un `pieza`: usa el
    /// barcode estable del producto (D35); sin caducidad si es externo.
    #[must_use]
    pub fn contenido(&self, nombre_producto: &str, codigo_producto: &Ean13) -> ContenidoEtiqueta {
        ContenidoEtiqueta {
            barcode: codigo_producto.get().to_string(),
            nombre: nombre_producto.to_string(),
            caducidad: self.caducidad.map(formatear_caducidad),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acceso::{Actor, Alcance};
    use crate::producto::OrigenProducto;

    fn instante(rfc: &str) -> Instante {
        Instante::desde_timestamp(rfc.parse().unwrap())
    }

    fn zona() -> ZonaHoraria {
        ZonaHoraria::nueva("America/Mexico_City").unwrap()
    }

    fn ctx(sucursal: Uuid) -> ContextoAcceso {
        ContextoAcceso::nuevo(
            Actor::Usuario(Uuid::new_v4()),
            [Permiso::Etiquetar].into_iter().collect(),
            Alcance::de([sucursal]),
        )
    }

    fn peso_variable() -> Producto {
        Producto::nuevo(
            "tripa de pollo",
            TipoProducto::PesoVariable,
            OrigenProducto::Matriz,
            None,
            None,
            Instante::ahora(),
        )
        .unwrap()
    }

    fn pieza(origen: OrigenProducto) -> Producto {
        let codigo = match origen {
            OrigenProducto::Matriz => crate::producto::CodigoBarras::Matriz(
                Ean13::generar_matriz(crate::barcode::PREFIJO_MATRIZ, 7).unwrap(),
            ),
            OrigenProducto::Externo => {
                crate::producto::CodigoBarras::Externo(Ean13::parse("7501059224827").unwrap())
            }
        };
        Producto::nuevo(
            "queso 500 g",
            TipoProducto::Pieza,
            origen,
            Some(codigo),
            None,
            Instante::ahora(),
        )
        .unwrap()
    }

    // -------- caducidad (D33) --------

    #[test]
    fn caducidad_default_nueve_meses() {
        // 13/01 mediodía CDMX + 9 meses → 13/10 del mismo año.
        let cad = calcular_caducidad(
            instante("2026-01-13T18:00:00Z"),
            &zona(),
            VidaUtil::default(),
        )
        .unwrap();
        assert_eq!(cad.to_string(), "2026-10-13");
    }

    #[test]
    fn caducidad_fin_de_mes_se_ajusta() {
        // 31/05 + 9 meses → 28/02 (2027 no es bisiesto).
        let cad = calcular_caducidad(
            instante("2026-05-31T18:00:00Z"),
            &zona(),
            VidaUtil::default(),
        )
        .unwrap();
        assert_eq!(cad.to_string(), "2027-02-28");
        // Año bisiesto: 31/05/2027 + 9 → 29/02/2028.
        let cad = calcular_caducidad(
            instante("2027-05-31T18:00:00Z"),
            &zona(),
            VidaUtil::default(),
        )
        .unwrap();
        assert_eq!(cad.to_string(), "2028-02-29");
    }

    #[test]
    fn caducidad_usa_el_dia_de_la_zona() {
        // 01/01 05:30 UTC aún es 31/12 en CDMX: la caducidad parte del día local.
        let cad = calcular_caducidad(
            instante("2026-01-01T05:30:00Z"),
            &zona(),
            VidaUtil::default(),
        )
        .unwrap();
        assert_eq!(cad.to_string(), "2026-09-30");
    }

    // -------- etiqueta por-ítem (D31/D32) --------

    #[test]
    fn pesadas_identicas_dan_etiquetas_distintas() {
        let p = peso_variable();
        let suc = Uuid::new_v4();
        let c = ctx(suc);
        let ahora = instante("2026-01-13T18:00:00Z");
        let a = Etiqueta::nueva(&c, &p, suc, &zona(), Gramos::new(1250), 1, ahora).unwrap();
        let b = Etiqueta::nueva(&c, &p, suc, &zona(), Gramos::new(1250), 2, ahora).unwrap();
        assert_ne!(a.codigo, b.codigo);
        assert_ne!(a.discriminador, b.discriminador);
    }

    #[test]
    fn etiqueta_congela_caducidad_y_exige_permiso() {
        let p = peso_variable();
        let suc = Uuid::new_v4();
        let ahora = instante("2026-01-13T18:00:00Z");
        let e = Etiqueta::nueva(&ctx(suc), &p, suc, &zona(), Gramos::new(800), 3, ahora).unwrap();
        assert_eq!(e.caducidad.to_string(), "2026-10-13");
        assert_eq!(e.estado, EstadoEtiqueta::Activa);
        // sin permiso `etiquetar` se rechaza
        let sin = ContextoAcceso::nuevo(
            Actor::Usuario(Uuid::new_v4()),
            [Permiso::Vender].into_iter().collect(),
            Alcance::de([suc]),
        );
        assert!(Etiqueta::nueva(&sin, &p, suc, &zona(), Gramos::new(800), 4, ahora).is_err());
    }

    #[test]
    fn pieza_no_lleva_etiqueta_por_item() {
        let p = pieza(OrigenProducto::Externo);
        let suc = Uuid::new_v4();
        let r = Etiqueta::nueva(
            &ctx(suc),
            &p,
            suc,
            &zona(),
            Gramos::new(500),
            1,
            Instante::ahora(),
        );
        assert!(r.is_err());
    }

    // -------- contenido físico (D32/D8) --------

    #[test]
    fn contenido_tres_elementos_sin_precio_ni_fecha() {
        let p = peso_variable();
        let suc = Uuid::new_v4();
        let ahora = instante("2026-01-13T18:00:00Z");
        let e = Etiqueta::nueva(&ctx(suc), &p, suc, &zona(), Gramos::new(1250), 9, ahora).unwrap();
        let contenido = e.contenido(&p.nombre);
        // los tres elementos, en el formato impreso DD/MM/AAAA
        assert_eq!(contenido.barcode, e.codigo.get());
        assert_eq!(contenido.nombre, "tripa de pollo");
        assert_eq!(contenido.caducidad.as_deref(), Some("13/10/2026"));
    }

    #[test]
    fn nombre_en_maximo_dos_lineas() {
        let contenido = ContenidoEtiqueta {
            barcode: "x".into(),
            nombre: "pechuga de pollo aplanada extra grande premium".into(),
            caducidad: None,
        };
        let lineas = contenido.lineas_nombre(16);
        assert!(lineas.len() <= 2);
        assert_eq!(lineas[0], "pechuga de pollo");
    }

    // -------- cajas (D35) --------

    #[test]
    fn caja_de_pesadas_agrupa_y_toma_la_caducidad_mas_proxima() {
        let p = peso_variable();
        let suc = Uuid::new_v4();
        let c = ctx(suc);
        let e1 = Etiqueta::nueva(
            &c,
            &p,
            suc,
            &zona(),
            Gramos::new(900),
            1,
            instante("2026-01-13T18:00:00Z"),
        )
        .unwrap();
        let e2 = Etiqueta::nueva(
            &c,
            &p,
            suc,
            &zona(),
            Gramos::new(1100),
            2,
            instante("2026-01-15T18:00:00Z"),
        )
        .unwrap();
        let caja =
            CajaEtiquetado::cerrar_pesadas(&c, &p, &[e1, e2], instante("2026-01-15T19:00:00Z"))
                .unwrap();
        assert_eq!(caja.cantidad, None);
        assert_eq!(caja.caducidad.unwrap().to_string(), "2026-10-13"); // la más próxima
    }

    #[test]
    fn caja_vacia_o_mezclada_se_rechaza() {
        let p = peso_variable();
        let suc = Uuid::new_v4();
        let c = ctx(suc);
        assert!(CajaEtiquetado::cerrar_pesadas(&c, &p, &[], Instante::ahora()).is_err());
        // etiqueta de otro producto
        let otro = peso_variable();
        let e = Etiqueta::nueva(
            &c,
            &otro,
            suc,
            &zona(),
            Gramos::new(500),
            1,
            Instante::ahora(),
        )
        .unwrap();
        assert!(CajaEtiquetado::cerrar_pesadas(&c, &p, &[e], Instante::ahora()).is_err());
    }

    #[test]
    fn caja_de_pieza_matriz_lleva_caducidad_y_de_externo_no() {
        let suc = Uuid::new_v4();
        let c = ctx(suc);
        let ahora = instante("2026-01-13T18:00:00Z");
        let matriz = pieza(OrigenProducto::Matriz);
        let caja = CajaEtiquetado::cerrar_pieza(&c, &matriz, suc, &zona(), 24, ahora).unwrap();
        assert_eq!(caja.cantidad, Some(24));
        assert!(caja.caducidad.is_some());
        let contenido = caja.contenido(&matriz.nombre, matriz.codigo.as_ref().unwrap().ean());
        assert_eq!(contenido.caducidad.as_deref(), Some("13/10/2026"));

        let externo = pieza(OrigenProducto::Externo);
        let caja = CajaEtiquetado::cerrar_pieza(&c, &externo, suc, &zona(), 12, ahora).unwrap();
        assert_eq!(caja.caducidad, None);
        let contenido = caja.contenido(&externo.nombre, externo.codigo.as_ref().unwrap().ean());
        assert_eq!(contenido.caducidad, None);
    }

    #[test]
    fn caja_de_pieza_rechaza_cantidad_no_positiva() {
        let suc = Uuid::new_v4();
        let c = ctx(suc);
        let p = pieza(OrigenProducto::Matriz);
        assert!(CajaEtiquetado::cerrar_pieza(&c, &p, suc, &zona(), 0, Instante::ahora()).is_err());
        assert!(CajaEtiquetado::cerrar_pieza(&c, &p, suc, &zona(), -3, Instante::ahora()).is_err());
    }
}
