//! Ciclo de caja: sesión con apertura y **cierre a ciegas**; el sistema calcula
//! y muestra, el humano decide (D13). El corte cerrado es **inmutable** (D19).
//!
//! `esperado = fondo + ventas_efectivo − gastos_autorizados`. Solo el efectivo
//! alimenta el arqueo; transferencia y depósito son informativos (D10). No hay
//! sanción automática: un gasto no autorizado no se resta y por eso **aparece
//! como faltante** por pura aritmética.

use uuid::Uuid;

use crate::acceso::{ContextoAcceso, Permiso};
use crate::error::ErrorDominio;
use crate::folio::Folio;
use crate::tiempo::Instante;
use crate::unidades::Centavos;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoSesion {
    Abierta,
    Cerrada,
}

/// Ventas del período clasificadas por método de pago.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResumenVentas {
    pub efectivo: Centavos,
    pub transferencia: Centavos,
    pub deposito: Centavos,
}

impl ResumenVentas {
    #[must_use]
    pub fn total(&self) -> Centavos {
        self.efectivo + self.transferencia + self.deposito
    }
}

/// Foto histórica del arqueo al cerrar la sesión. Inmutable (D19).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corte {
    pub folio: Folio,
    pub fondo_apertura: Centavos,
    pub ventas: ResumenVentas,
    pub gastos_autorizados: Centavos,
    pub esperado: Centavos,
    pub contado: Centavos,
    /// `contado − esperado`: negativo es faltante, positivo sobrante.
    pub diferencia: Centavos,
}

impl Corte {
    /// Venta neta en efectivo: las ventas cobradas en efectivo (el fondo NO es venta).
    #[must_use]
    pub fn venta_neta_efectivo(&self) -> Centavos {
        self.ventas.efectivo
    }

    #[must_use]
    pub fn es_faltante(&self) -> bool {
        self.diferencia.get() < 0
    }
}

/// Sesión de caja: una por sucursal (un cajero = una caja). Define "el día".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SesionCaja {
    pub id: Uuid,
    pub sucursal: Uuid,
    pub cajero: Uuid,
    pub fondo_apertura: Centavos,
    pub abierta_en: Instante,
    pub estado: EstadoSesion,
    pub corte: Option<Corte>,
    pub cerrada_en: Option<Instante>,
    pub actualizado: Instante,
}

impl SesionCaja {
    /// Abre la sesión con el fondo autodeclarado. La regla de "a lo sumo una
    /// abierta por sucursal" la hace cumplir el almacenamiento (ve las demás).
    pub fn abrir(
        ctx: &ContextoAcceso,
        sucursal: Uuid,
        fondo: Centavos,
        ahora: Instante,
    ) -> Result<Self, ErrorDominio> {
        ctx.requiere_en(Permiso::OperarCaja, sucursal)?;
        let cajero = ctx.usuario()?;
        if fondo.get() < 0 {
            return Err(ErrorDominio::Invalido(
                "el fondo de apertura no puede ser negativo".into(),
            ));
        }
        Ok(SesionCaja {
            id: Uuid::new_v4(),
            sucursal,
            cajero,
            fondo_apertura: fondo,
            abierta_en: ahora,
            estado: EstadoSesion::Abierta,
            corte: None,
            cerrada_en: None,
            actualizado: ahora,
        })
    }

    /// Cierra la sesión (corte). El cajero captura solo su efectivo contado; no
    /// se le devuelve el corte (**cierre a ciegas**) — para verlo hace falta
    /// `VerConciliacion`. Una sesión cerrada no se re-cierra (inmutable, D19).
    pub fn cerrar(
        &mut self,
        ctx: &ContextoAcceso,
        contado: Centavos,
        ventas: ResumenVentas,
        gastos_autorizados: Centavos,
        folio: Folio,
        ahora: Instante,
    ) -> Result<(), ErrorDominio> {
        ctx.requiere_en(Permiso::OperarCaja, self.sucursal)?;
        if self.estado == EstadoSesion::Cerrada {
            return Err(ErrorDominio::TransicionInvalida(
                "la sesión ya está cerrada".into(),
            ));
        }
        let esperado = self.fondo_apertura + ventas.efectivo - gastos_autorizados;
        self.corte = Some(Corte {
            folio,
            fondo_apertura: self.fondo_apertura,
            ventas,
            gastos_autorizados,
            esperado,
            contado,
            diferencia: contado - esperado,
        });
        self.estado = EstadoSesion::Cerrada;
        self.cerrada_en = Some(ahora);
        self.actualizado = ahora;
        Ok(())
    }

    /// Revela la conciliación (esperado/diferencia): permiso del administrador,
    /// no del cajero (cierre a ciegas).
    pub fn ver_conciliacion(&self, ctx: &ContextoAcceso) -> Result<&Corte, ErrorDominio> {
        ctx.requiere_en(Permiso::VerConciliacion, self.sucursal)?;
        self.corte
            .as_ref()
            .ok_or_else(|| ErrorDominio::Regla("la sesión aún no tiene corte".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acceso::{Actor, Alcance, ContextoAcceso};
    use crate::folio::{Folio, TipoDocumento};
    use crate::sucursal::CodigoSucursal;

    fn ctx(permisos: &[Permiso], sucursal: Uuid) -> ContextoAcceso {
        ContextoAcceso::nuevo(
            Actor::Usuario(Uuid::new_v4()),
            permisos.iter().copied().collect(),
            Alcance::de([sucursal]),
        )
    }

    fn folio_corte() -> Folio {
        Folio::componer(
            &CodigoSucursal::nueva("SNJ").unwrap(),
            TipoDocumento::Corte,
            56,
        )
    }

    #[test]
    fn esperado_y_venta_neta() {
        let suc = Uuid::new_v4();
        let c = ctx(&[Permiso::OperarCaja, Permiso::VerConciliacion], suc);
        let mut s =
            SesionCaja::abrir(&c, suc, Centavos::de_pesos(500, 0), Instante::ahora()).unwrap();
        let ventas = ResumenVentas {
            efectivo: Centavos::de_pesos(8000, 0),
            ..Default::default()
        };
        s.cerrar(
            &c,
            Centavos::de_pesos(7300, 0),
            ventas,
            Centavos::de_pesos(1200, 0),
            folio_corte(),
            Instante::ahora(),
        )
        .unwrap();
        let corte = s.ver_conciliacion(&c).unwrap();
        // esperado = 500 + 8000 - 1200 = 7300 ; venta neta = 8000
        assert_eq!(corte.esperado, Centavos::de_pesos(7300, 0));
        assert_eq!(corte.venta_neta_efectivo(), Centavos::de_pesos(8000, 0));
        assert_eq!(corte.diferencia, Centavos::CERO);
    }

    #[test]
    fn transferencia_no_entra_al_arqueo() {
        let suc = Uuid::new_v4();
        let c = ctx(&[Permiso::OperarCaja, Permiso::VerConciliacion], suc);
        let mut s = SesionCaja::abrir(&c, suc, Centavos::CERO, Instante::ahora()).unwrap();
        let ventas = ResumenVentas {
            efectivo: Centavos::de_pesos(8000, 0),
            transferencia: Centavos::de_pesos(3000, 0),
            deposito: Centavos::CERO,
        };
        s.cerrar(
            &c,
            Centavos::de_pesos(8000, 0),
            ventas,
            Centavos::CERO,
            folio_corte(),
            Instante::ahora(),
        )
        .unwrap();
        let corte = s.ver_conciliacion(&c).unwrap();
        assert_eq!(corte.ventas.total(), Centavos::de_pesos(11000, 0)); // muestra total
        assert_eq!(corte.esperado, Centavos::de_pesos(8000, 0)); // pero solo efectivo al arqueo
    }

    #[test]
    fn faltante_sin_sancion() {
        let suc = Uuid::new_v4();
        let c = ctx(&[Permiso::OperarCaja, Permiso::VerConciliacion], suc);
        let mut s = SesionCaja::abrir(&c, suc, Centavos::CERO, Instante::ahora()).unwrap();
        let ventas = ResumenVentas {
            efectivo: Centavos::de_pesos(7300, 0),
            ..Default::default()
        };
        s.cerrar(
            &c,
            Centavos::de_pesos(7250, 0),
            ventas,
            Centavos::CERO,
            folio_corte(),
            Instante::ahora(),
        )
        .unwrap();
        let corte = s.ver_conciliacion(&c).unwrap();
        assert!(corte.es_faltante());
        assert_eq!(corte.diferencia, Centavos::de_pesos(-50, 0));
    }

    #[test]
    fn cajero_no_ve_conciliacion() {
        let suc = Uuid::new_v4();
        let cajero = ctx(&[Permiso::OperarCaja], suc); // sin VerConciliacion
        let mut s = SesionCaja::abrir(&cajero, suc, Centavos::CERO, Instante::ahora()).unwrap();
        s.cerrar(
            &cajero,
            Centavos::de_pesos(100, 0),
            ResumenVentas::default(),
            Centavos::CERO,
            folio_corte(),
            Instante::ahora(),
        )
        .unwrap();
        assert!(s.ver_conciliacion(&cajero).is_err());
    }

    #[test]
    fn corte_cerrado_es_inmutable() {
        let suc = Uuid::new_v4();
        let c = ctx(&[Permiso::OperarCaja], suc);
        let mut s = SesionCaja::abrir(&c, suc, Centavos::CERO, Instante::ahora()).unwrap();
        s.cerrar(
            &c,
            Centavos::CERO,
            ResumenVentas::default(),
            Centavos::CERO,
            folio_corte(),
            Instante::ahora(),
        )
        .unwrap();
        // segundo cierre rechazado
        assert!(
            s.cerrar(
                &c,
                Centavos::CERO,
                ResumenVentas::default(),
                Centavos::CERO,
                folio_corte(),
                Instante::ahora()
            )
            .is_err()
        );
    }
}
