//! Gastos: nacen **no autorizados** y sujetos a aprobación (D4). Su autorización
//! la determina la `Aprobacion` ligada, no un flag propio (fuente única).

use uuid::Uuid;

use crate::acceso::{ContextoAcceso, Permiso};
use crate::error::ErrorDominio;
use crate::folio::Folio;
use crate::tiempo::Instante;
use crate::unidades::Centavos;

/// Un gasto registrado en una sesión de caja. El `folio` (tipo `G`) lo emite el
/// generador de la capa de almacenamiento y se asigna al confirmarse (D18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gasto {
    pub id: Uuid,
    pub folio: Folio,
    pub monto: Centavos,
    pub concepto: String,
    pub cajero: Uuid,
    pub sucursal: Uuid,
    pub sesion: Uuid,
    pub registrado: Instante,
    pub actualizado: Instante,
}

impl Gasto {
    /// Registra un gasto en la sucursal indicada. Exige `RegistrarGasto` +
    /// alcance; el cajero es el usuario en sesión.
    pub fn registrar(
        ctx: &ContextoAcceso,
        folio: Folio,
        monto: Centavos,
        concepto: &str,
        sucursal: Uuid,
        sesion: Uuid,
        ahora: Instante,
    ) -> Result<Self, ErrorDominio> {
        ctx.requiere_en(Permiso::RegistrarGasto, sucursal)?;
        let cajero = ctx.usuario()?;
        if monto.get() <= 0 {
            return Err(ErrorDominio::Invalido(
                "el monto del gasto debe ser positivo".into(),
            ));
        }
        if concepto.trim().is_empty() {
            return Err(ErrorDominio::Invalido(
                "el concepto del gasto es obligatorio".into(),
            ));
        }
        Ok(Gasto {
            id: Uuid::new_v4(),
            folio,
            monto,
            concepto: concepto.trim().to_string(),
            cajero,
            sucursal,
            sesion,
            registrado: ahora,
            actualizado: ahora,
        })
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

    fn folio() -> Folio {
        Folio::componer(
            &CodigoSucursal::nueva("SRO").unwrap(),
            TipoDocumento::Gasto,
            204,
        )
    }

    #[test]
    fn registrar_exige_permiso() {
        let suc = Uuid::new_v4();
        let sin = ctx(&[Permiso::Vender], suc);
        assert!(
            Gasto::registrar(
                &sin,
                folio(),
                Centavos::de_pesos(2000, 0),
                "publicidad",
                suc,
                Uuid::new_v4(),
                Instante::ahora()
            )
            .is_err()
        );
    }

    #[test]
    fn registrar_valido() {
        let suc = Uuid::new_v4();
        let c = ctx(&[Permiso::RegistrarGasto], suc);
        let g = Gasto::registrar(
            &c,
            folio(),
            Centavos::de_pesos(2000, 0),
            "publicidad",
            suc,
            Uuid::new_v4(),
            Instante::ahora(),
        )
        .unwrap();
        assert_eq!(g.monto, Centavos::de_pesos(2000, 0));
        assert_eq!(g.folio.get(), "SROG204");
    }

    #[test]
    fn monto_no_positivo_se_rechaza() {
        let suc = Uuid::new_v4();
        let c = ctx(&[Permiso::RegistrarGasto], suc);
        assert!(
            Gasto::registrar(
                &c,
                folio(),
                Centavos::CERO,
                "x",
                suc,
                Uuid::new_v4(),
                Instante::ahora()
            )
            .is_err()
        );
    }
}
