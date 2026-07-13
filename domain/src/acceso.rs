//! Acceso: dos ejes ortogonales — **permiso** (*qué*) y **alcance** (*dónde*) — y
//! el `ContextoAcceso` que cada operación exige (D3).
//!
//! El chequeo es `tiene(permiso) ∧ alcance_cubre(sucursal)` para permisos
//! por-sucursal, y solo `tiene(permiso)` para los de-sistema. Con un **único
//! alcance por usuario** no hay fuga por unión de alcances.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ErrorDominio;

/// Quién ejecuta una operación: un usuario, o el propio sistema (bootstrap y
/// operaciones automáticas), de modo que ninguna escritura quede sin atribuir (D3/D11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Actor {
    Usuario(Uuid),
    Sistema,
}

impl fmt::Display for Actor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Actor::Usuario(id) => write!(f, "usuario:{id}"),
            Actor::Sistema => write!(f, "sistema"),
        }
    }
}

/// Cómo se evalúa un permiso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClasePermiso {
    /// Requiere alcance sobre una sucursal objetivo.
    PorSucursal,
    /// Acción global; basta portar el permiso.
    DeSistema,
}

/// Catálogo de permisos atómicos (nivel capacidad). Extensible: sumar una
/// variante no toca las existentes (D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Permiso {
    // -- por-sucursal --
    Vender,
    EditarPrecio,
    RegistrarGasto,
    AutorizarGasto,
    OperarCaja,
    VerConciliacion,
    AjustarInventario,
    VerInventario,
    Etiquetar,
    VerNotificaciones,
    // -- de-sistema --
    AutoaprobarGasto,
    GestionarUsuarios,
    GestionarRoles,
    GestionarSucursales,
    GestionarProductos,
}

impl Permiso {
    #[must_use]
    pub fn clase(self) -> ClasePermiso {
        use Permiso::{
            AjustarInventario, AutoaprobarGasto, AutorizarGasto, EditarPrecio, Etiquetar,
            GestionarProductos, GestionarRoles, GestionarSucursales, GestionarUsuarios, OperarCaja,
            RegistrarGasto, Vender, VerConciliacion, VerInventario, VerNotificaciones,
        };
        match self {
            Vender | EditarPrecio | RegistrarGasto | AutorizarGasto | OperarCaja
            | VerConciliacion | AjustarInventario | VerInventario | Etiquetar
            | VerNotificaciones => ClasePermiso::PorSucursal,
            AutoaprobarGasto | GestionarUsuarios | GestionarRoles | GestionarSucursales
            | GestionarProductos => ClasePermiso::DeSistema,
        }
    }
}

impl fmt::Display for Permiso {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Conjunto de sucursales sobre las que opera un usuario: una, varias, o
/// **todas** (nivel matriz). Es del usuario, no de sus roles ni permisos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Alcance {
    /// Nivel matriz: todas las sucursales. No concede permisos por sí mismo.
    Todas,
    Sucursales(BTreeSet<Uuid>),
}

impl Alcance {
    #[must_use]
    pub fn de(sucursales: impl IntoIterator<Item = Uuid>) -> Self {
        Alcance::Sucursales(sucursales.into_iter().collect())
    }

    #[must_use]
    pub fn cubre(&self, sucursal: Uuid) -> bool {
        match self {
            Alcance::Todas => true,
            Alcance::Sucursales(s) => s.contains(&sucursal),
        }
    }
}

/// Reja que cada repositorio exige: quién actúa, con qué permisos y sobre qué
/// alcance. Es el punto único donde se aplican aislamiento y auditoría.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextoAcceso {
    pub actor: Actor,
    pub permisos: BTreeSet<Permiso>,
    pub alcance: Alcance,
}

impl ContextoAcceso {
    #[must_use]
    pub fn nuevo(actor: Actor, permisos: BTreeSet<Permiso>, alcance: Alcance) -> Self {
        ContextoAcceso {
            actor,
            permisos,
            alcance,
        }
    }

    /// Contexto del `sistema`: todos los permisos y alcance total. Sostiene el
    /// bootstrap y las operaciones automáticas sin puentear el enforcement (D3).
    #[must_use]
    pub fn sistema() -> Self {
        let todos = [
            Permiso::Vender,
            Permiso::EditarPrecio,
            Permiso::RegistrarGasto,
            Permiso::AutorizarGasto,
            Permiso::OperarCaja,
            Permiso::VerConciliacion,
            Permiso::AjustarInventario,
            Permiso::VerInventario,
            Permiso::Etiquetar,
            Permiso::VerNotificaciones,
            Permiso::AutoaprobarGasto,
            Permiso::GestionarUsuarios,
            Permiso::GestionarRoles,
            Permiso::GestionarSucursales,
            Permiso::GestionarProductos,
        ];
        ContextoAcceso {
            actor: Actor::Sistema,
            permisos: todos.into_iter().collect(),
            alcance: Alcance::Todas,
        }
    }

    #[must_use]
    pub fn tiene(&self, permiso: Permiso) -> bool {
        self.permisos.contains(&permiso)
    }

    /// El id del usuario en sesión, o error si el actor es el `sistema`.
    pub fn usuario(&self) -> Result<Uuid, ErrorDominio> {
        match self.actor {
            Actor::Usuario(id) => Ok(id),
            Actor::Sistema => Err(ErrorDominio::RequiereUsuario),
        }
    }

    /// Exige un permiso **de-sistema**.
    pub fn requiere(&self, permiso: Permiso) -> Result<(), ErrorDominio> {
        debug_assert_eq!(
            permiso.clase(),
            ClasePermiso::DeSistema,
            "usa `requiere_en` para permisos por-sucursal"
        );
        if self.tiene(permiso) {
            Ok(())
        } else {
            Err(ErrorDominio::PermisoDenegado(permiso))
        }
    }

    /// Exige un permiso **por-sucursal**: portarlo y que el alcance cubra la sucursal.
    pub fn requiere_en(&self, permiso: Permiso, sucursal: Uuid) -> Result<(), ErrorDominio> {
        debug_assert_eq!(
            permiso.clase(),
            ClasePermiso::PorSucursal,
            "usa `requiere` para permisos de-sistema"
        );
        if !self.tiene(permiso) {
            return Err(ErrorDominio::PermisoDenegado(permiso));
        }
        if !self.alcance.cubre(sucursal) {
            return Err(ErrorDominio::FueraDeAlcance);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(permisos: &[Permiso], alcance: Alcance) -> ContextoAcceso {
        ContextoAcceso::nuevo(
            Actor::Usuario(Uuid::new_v4()),
            permisos.iter().copied().collect(),
            alcance,
        )
    }

    #[test]
    fn clase_de_permisos() {
        assert_eq!(Permiso::Vender.clase(), ClasePermiso::PorSucursal);
        assert_eq!(Permiso::AutoaprobarGasto.clase(), ClasePermiso::DeSistema);
        assert_eq!(Permiso::GestionarRoles.clase(), ClasePermiso::DeSistema);
    }

    #[test]
    fn por_sucursal_requiere_permiso_y_alcance() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = ctx(&[Permiso::Vender], Alcance::de([a]));
        assert!(c.requiere_en(Permiso::Vender, a).is_ok());
        assert_eq!(
            c.requiere_en(Permiso::Vender, b),
            Err(ErrorDominio::FueraDeAlcance)
        );
        assert_eq!(
            c.requiere_en(Permiso::EditarPrecio, a),
            Err(ErrorDominio::PermisoDenegado(Permiso::EditarPrecio))
        );
    }

    #[test]
    fn alcance_todas_no_concede_permisos() {
        let c = ctx(&[], Alcance::Todas);
        // ve todo, pero sin `AutorizarGasto` no autoriza en ninguna sucursal
        assert_eq!(
            c.requiere_en(Permiso::AutorizarGasto, Uuid::new_v4()),
            Err(ErrorDominio::PermisoDenegado(Permiso::AutorizarGasto))
        );
    }

    #[test]
    fn de_sistema_no_mira_sucursal() {
        let c = ctx(&[Permiso::GestionarRoles], Alcance::de([]));
        assert!(c.requiere(Permiso::GestionarRoles).is_ok());
    }

    #[test]
    fn sistema_tiene_todo() {
        let s = ContextoAcceso::sistema();
        assert!(s.requiere(Permiso::GestionarProductos).is_ok());
        assert!(s.requiere_en(Permiso::Vender, Uuid::new_v4()).is_ok());
        assert!(s.usuario().is_err());
    }
}
