//! Envío entre sucursales (D38–D43): documento citable con folio y ciclo
//! `preparado → enviado → recibido` (cancelable antes de enviar), **sin flujo
//! de aprobación** (D4). Un extremo siempre es la matriz (D39). Los efectos de
//! inventario son asimétricos (D40): la matriz jamás postea movimientos.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::acceso::{ContextoAcceso, Permiso};
use crate::error::ErrorDominio;
use crate::folio::Folio;
use crate::inventario::{Cantidad, EstadoMovimiento, MovimientoInventario, TipoMovimiento};
use crate::sucursal::TipoSucursal;
use crate::tiempo::Instante;

/// Estado del envío. `Cancelado` solo se alcanza desde `Preparado` y conserva
/// el folio (D18: sin reutilización).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstadoEnvio {
    Preparado,
    Enviado,
    Recibido,
    Cancelado,
}

/// El documento de envío. Su contenido (cajas y etiquetas sueltas) vive ligado
/// por `envio_id` en la capa de datos (D42); aquí viven identidad y ciclo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envio {
    pub id: Uuid,
    pub folio: Folio,
    pub origen: Uuid,
    pub destino: Uuid,
    pub estado: EstadoEnvio,
    pub preparado_en: Instante,
    pub enviado_en: Option<Instante>,
    pub recibido_en: Option<Instante>,
    /// Anotación de faltantes al recibir (D41); `None` si la recepción cuadró.
    pub discrepancia: Option<String>,
    pub actualizado: Instante,
}

impl Envio {
    /// Prepara un envío. Exige `enviar` + alcance sobre el **origen**, que
    /// origen y destino difieran y que **un extremo sea la matriz** (D39):
    /// matriz→expendio (surtido) o expendio→matriz (devolución).
    pub fn preparar(
        ctx: &ContextoAcceso,
        folio: Folio,
        origen: Uuid,
        tipo_origen: TipoSucursal,
        destino: Uuid,
        tipo_destino: TipoSucursal,
        ahora: Instante,
    ) -> Result<Self, ErrorDominio> {
        ctx.requiere_en(Permiso::Enviar, origen)?;
        if origen == destino {
            return Err(ErrorDominio::Regla(
                "el origen y el destino del envío deben ser distintos".into(),
            ));
        }
        if tipo_origen != TipoSucursal::Matriz && tipo_destino != TipoSucursal::Matriz {
            return Err(ErrorDominio::Regla(
                "un extremo del envío debe ser la matriz; no hay traspasos entre expendios".into(),
            ));
        }
        Ok(Envio {
            id: Uuid::new_v4(),
            folio,
            origen,
            destino,
            estado: EstadoEnvio::Preparado,
            preparado_en: ahora,
            enviado_en: None,
            recibido_en: None,
            discrepancia: None,
            actualizado: ahora,
        })
    }

    /// Marca el envío como enviado (la mercancía sale físicamente). Solo desde
    /// `preparado`; exige `enviar` + alcance sobre el origen.
    pub fn marcar_enviado(
        &mut self,
        ctx: &ContextoAcceso,
        ahora: Instante,
    ) -> Result<(), ErrorDominio> {
        ctx.requiere_en(Permiso::Enviar, self.origen)?;
        if self.estado != EstadoEnvio::Preparado {
            return Err(ErrorDominio::TransicionInvalida(
                "solo un envío preparado puede marcarse enviado".into(),
            ));
        }
        self.estado = EstadoEnvio::Enviado;
        self.enviado_en = Some(ahora);
        self.actualizado = ahora;
        Ok(())
    }

    /// Cancela el envío (solo en `preparado`); el contenido se libera en la
    /// capa de datos (D42). Conserva su folio.
    pub fn cancelar(&mut self, ctx: &ContextoAcceso, ahora: Instante) -> Result<(), ErrorDominio> {
        ctx.requiere_en(Permiso::Enviar, self.origen)?;
        if self.estado != EstadoEnvio::Preparado {
            return Err(ErrorDominio::TransicionInvalida(
                "solo un envío preparado puede cancelarse".into(),
            ));
        }
        self.estado = EstadoEnvio::Cancelado;
        self.actualizado = ahora;
        Ok(())
    }

    /// Marca el envío como recibido, con la anotación de discrepancia si hubo
    /// faltantes (D41). Solo desde `enviado`; exige `recibir` + alcance sobre
    /// el **destino** (no se recibe lo no enviado; no se re-recibe).
    pub fn marcar_recibido(
        &mut self,
        ctx: &ContextoAcceso,
        discrepancia: Option<String>,
        ahora: Instante,
    ) -> Result<(), ErrorDominio> {
        ctx.requiere_en(Permiso::Recibir, self.destino)?;
        if self.estado != EstadoEnvio::Enviado {
            return Err(ErrorDominio::TransicionInvalida(
                "solo un envío enviado puede recibirse".into(),
            ));
        }
        self.estado = EstadoEnvio::Recibido;
        self.recibido_en = Some(ahora);
        self.discrepancia = discrepancia;
        self.actualizado = ahora;
        Ok(())
    }
}

/// Acumula cantidades por producto (etiquetas en gramos, cajas de pieza en
/// unidades) para postear un movimiento por producto (D40). Rechaza mezclar
/// unidades de un mismo producto (imposible por construcción, pero se valida).
pub fn deltas_por_producto(
    items: impl IntoIterator<Item = (Uuid, Cantidad)>,
) -> Result<BTreeMap<Uuid, Cantidad>, ErrorDominio> {
    let mut deltas: BTreeMap<Uuid, Cantidad> = BTreeMap::new();
    for (producto, cantidad) in items {
        let nuevo = match deltas.get(&producto) {
            Some(previa) => previa.sumar(cantidad)?,
            None => cantidad,
        };
        deltas.insert(producto, nuevo);
    }
    Ok(deltas)
}

/// El asiento de **salida** de una devolución al marcarse enviada (D40): delta
/// negativo en el expendio origen, citando el folio del envío. La no-negatividad
/// del saldo la valida quien lo aplica (D25).
#[must_use]
pub fn movimiento_salida(
    producto: Uuid,
    sucursal: Uuid,
    cantidad: Cantidad,
    folio: &Folio,
    ahora: Instante,
) -> MovimientoInventario {
    asiento(
        producto,
        sucursal,
        TipoMovimiento::Envio,
        cantidad.negada(),
        folio,
        ahora,
    )
}

/// El asiento de **entrada** al recibir un surtido en un expendio (D40): delta
/// positivo con lo presente, citando el folio del envío.
#[must_use]
pub fn movimiento_recepcion(
    producto: Uuid,
    sucursal: Uuid,
    cantidad: Cantidad,
    folio: &Folio,
    ahora: Instante,
) -> MovimientoInventario {
    asiento(
        producto,
        sucursal,
        TipoMovimiento::Recepcion,
        cantidad,
        folio,
        ahora,
    )
}

fn asiento(
    producto: Uuid,
    sucursal: Uuid,
    tipo: TipoMovimiento,
    delta: Cantidad,
    folio: &Folio,
    ahora: Instante,
) -> MovimientoInventario {
    MovimientoInventario {
        id: Uuid::new_v4(),
        producto,
        sucursal,
        tipo,
        delta,
        motivo: Some(folio.get().to_string()),
        estado: EstadoMovimiento::Aplicado,
        registrado: ahora,
        actualizado: ahora,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acceso::{Actor, Alcance};
    use crate::folio::TipoDocumento;
    use crate::sucursal::CodigoSucursal;
    use crate::unidades::Gramos;

    fn folio() -> Folio {
        Folio::componer(
            &CodigoSucursal::nueva("MAT").unwrap(),
            TipoDocumento::Envio,
            1,
        )
    }

    fn ctx(permisos: &[Permiso], sucursal: Uuid) -> ContextoAcceso {
        ContextoAcceso::nuevo(
            Actor::Usuario(Uuid::new_v4()),
            permisos.iter().copied().collect(),
            Alcance::de([sucursal]),
        )
    }

    fn preparado(origen: Uuid, destino: Uuid) -> Envio {
        Envio::preparar(
            &ctx(&[Permiso::Enviar], origen),
            folio(),
            origen,
            TipoSucursal::Matriz,
            destino,
            TipoSucursal::Expendio,
            Instante::ahora(),
        )
        .unwrap()
    }

    #[test]
    fn surtido_y_devolucion_validos_lateral_rechazado() {
        let m = Uuid::new_v4();
        let e = Uuid::new_v4();
        let c = ctx(&[Permiso::Enviar], m);
        assert!(
            Envio::preparar(
                &c,
                folio(),
                m,
                TipoSucursal::Matriz,
                e,
                TipoSucursal::Expendio,
                Instante::ahora()
            )
            .is_ok()
        );
        let c = ctx(&[Permiso::Enviar], e);
        assert!(
            Envio::preparar(
                &c,
                folio(),
                e,
                TipoSucursal::Expendio,
                m,
                TipoSucursal::Matriz,
                Instante::ahora()
            )
            .is_ok()
        );
        // expendio → expendio se rechaza (D39)
        let otro = Uuid::new_v4();
        assert!(
            Envio::preparar(
                &c,
                folio(),
                e,
                TipoSucursal::Expendio,
                otro,
                TipoSucursal::Expendio,
                Instante::ahora()
            )
            .is_err()
        );
        // origen == destino se rechaza
        let c = ctx(&[Permiso::Enviar], m);
        assert!(
            Envio::preparar(
                &c,
                folio(),
                m,
                TipoSucursal::Matriz,
                m,
                TipoSucursal::Matriz,
                Instante::ahora()
            )
            .is_err()
        );
    }

    #[test]
    fn preparar_exige_permiso_sobre_el_origen() {
        let m = Uuid::new_v4();
        let e = Uuid::new_v4();
        // permiso sobre el destino no basta
        let c = ctx(&[Permiso::Enviar], e);
        assert!(
            Envio::preparar(
                &c,
                folio(),
                m,
                TipoSucursal::Matriz,
                e,
                TipoSucursal::Expendio,
                Instante::ahora()
            )
            .is_err()
        );
        let sin = ctx(&[Permiso::Vender], m);
        assert!(
            Envio::preparar(
                &sin,
                folio(),
                m,
                TipoSucursal::Matriz,
                e,
                TipoSucursal::Expendio,
                Instante::ahora()
            )
            .is_err()
        );
    }

    #[test]
    fn ciclo_completo_y_transiciones_invalidas() {
        let m = Uuid::new_v4();
        let e = Uuid::new_v4();
        let mut envio = preparado(m, e);
        let c_origen = ctx(&[Permiso::Enviar], m);
        let c_destino = ctx(&[Permiso::Recibir], e);

        // recibir sin haber enviado se rechaza
        assert!(
            envio
                .marcar_recibido(&c_destino, None, Instante::ahora())
                .is_err()
        );
        envio.marcar_enviado(&c_origen, Instante::ahora()).unwrap();
        assert_eq!(envio.estado, EstadoEnvio::Enviado);
        // cancelar tras enviar se rechaza
        assert!(envio.cancelar(&c_origen, Instante::ahora()).is_err());
        // recibir exige permiso sobre el destino
        assert!(
            envio
                .marcar_recibido(&c_origen, None, Instante::ahora())
                .is_err()
        );
        envio
            .marcar_recibido(&c_destino, None, Instante::ahora())
            .unwrap();
        assert_eq!(envio.estado, EstadoEnvio::Recibido);
        assert!(envio.recibido_en.is_some() && envio.discrepancia.is_none());
        // re-recibir se rechaza
        assert!(
            envio
                .marcar_recibido(&c_destino, None, Instante::ahora())
                .is_err()
        );
    }

    #[test]
    fn cancelar_solo_preparado_y_conserva_folio() {
        let m = Uuid::new_v4();
        let e = Uuid::new_v4();
        let mut envio = preparado(m, e);
        let c = ctx(&[Permiso::Enviar], m);
        envio.cancelar(&c, Instante::ahora()).unwrap();
        assert_eq!(envio.estado, EstadoEnvio::Cancelado);
        assert_eq!(envio.folio.get(), "MATE1");
    }

    #[test]
    fn deltas_acumulan_por_producto() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let deltas = deltas_por_producto([
            (a, Cantidad::Gramos(Gramos::new(900))),
            (b, Cantidad::Unidades(24)),
            (a, Cantidad::Gramos(Gramos::new(1100))),
        ])
        .unwrap();
        assert_eq!(deltas[&a], Cantidad::Gramos(Gramos::new(2000)));
        assert_eq!(deltas[&b], Cantidad::Unidades(24));
    }

    #[test]
    fn asientos_citan_el_folio_y_firman_el_delta() {
        let prod = Uuid::new_v4();
        let suc = Uuid::new_v4();
        let f = folio();
        let salida = movimiento_salida(prod, suc, Cantidad::Unidades(24), &f, Instante::ahora());
        assert_eq!(salida.tipo, TipoMovimiento::Envio);
        assert_eq!(salida.delta, Cantidad::Unidades(-24));
        assert_eq!(salida.motivo.as_deref(), Some("MATE1"));
        let entrada = movimiento_recepcion(
            prod,
            suc,
            Cantidad::Gramos(Gramos::new(2500)),
            &f,
            Instante::ahora(),
        );
        assert_eq!(entrada.tipo, TipoMovimiento::Recepcion);
        assert_eq!(entrada.delta, Cantidad::Gramos(Gramos::new(2500)));
    }
}
