//! Usuarios, credenciales y roles componibles.
//!
//! La credencial es uniforme: usuario + contraseña **hasheada** (argon2), nunca
//! en texto plano, validable localmente sin red (D16). Los roles son bundles
//! editables de permisos atómicos (D2); los permisos efectivos del usuario son
//! la **unión** de los de sus roles. El **alcance** es del usuario, aparte de
//! los roles (D3).

use std::collections::BTreeSet;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use rand_core::OsRng;
use uuid::Uuid;

use crate::acceso::{Alcance, Permiso};
use crate::error::ErrorDominio;
use crate::tiempo::Instante;

/// Credencial hasheada (cadena PHC de argon2). Nunca guarda la contraseña en claro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credencial {
    phc: String,
}

impl Credencial {
    /// Deriva el hash argon2 de una contraseña.
    pub fn nueva(contrasena: &str) -> Result<Self, ErrorDominio> {
        if contrasena.len() < 4 {
            return Err(ErrorDominio::Invalido(
                "la contraseña es demasiado corta".into(),
            ));
        }
        let salt = SaltString::generate(&mut OsRng);
        let phc = Argon2::default()
            .hash_password(contrasena.as_bytes(), &salt)
            .map_err(|e| ErrorDominio::Invalido(format!("no se pudo hashear la contraseña: {e}")))?
            .to_string();
        Ok(Credencial { phc })
    }

    /// Reconstruye desde el hash persistido (sin re-hashear).
    #[must_use]
    pub fn desde_hash(phc: String) -> Self {
        Credencial { phc }
    }

    #[must_use]
    pub fn hash(&self) -> &str {
        &self.phc
    }

    /// Verifica una contraseña contra el hash almacenado.
    #[must_use]
    pub fn verificar(&self, contrasena: &str) -> bool {
        let Ok(parsed) = PasswordHash::new(&self.phc) else {
            return false;
        };
        Argon2::default()
            .verify_password(contrasena.as_bytes(), &parsed)
            .is_ok()
    }
}

/// Rol: conjunto editable de permisos, sin catálogo cerrado. Editarlo en
/// caliente afecta a quienes ya lo tienen asignado (D2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rol {
    pub id: Uuid,
    pub nombre: String,
    pub permisos: BTreeSet<Permiso>,
    pub activo: bool,
    pub actualizado: Instante,
}

impl Rol {
    pub fn nuevo(
        nombre: &str,
        permisos: impl IntoIterator<Item = Permiso>,
        ahora: Instante,
    ) -> Result<Self, ErrorDominio> {
        if nombre.trim().is_empty() {
            return Err(ErrorDominio::Invalido(
                "el nombre del rol es obligatorio".into(),
            ));
        }
        Ok(Rol {
            id: Uuid::new_v4(),
            nombre: nombre.trim().to_string(),
            permisos: permisos.into_iter().collect(),
            activo: true,
            actualizado: ahora,
        })
    }

    pub fn agregar_permiso(&mut self, permiso: Permiso, ahora: Instante) {
        self.permisos.insert(permiso);
        self.actualizado = ahora;
    }

    pub fn quitar_permiso(&mut self, permiso: Permiso, ahora: Instante) {
        self.permisos.remove(&permiso);
        self.actualizado = ahora;
    }
}

/// Usuario del sistema. Su alcance vive aquí (no en los roles).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usuario {
    pub id: Uuid,
    pub usuario: String,
    pub credencial: Credencial,
    pub roles: Vec<Uuid>,
    pub alcance: Alcance,
    pub activo: bool,
    pub actualizado: Instante,
}

impl Usuario {
    pub fn nuevo(
        usuario: &str,
        credencial: Credencial,
        roles: Vec<Uuid>,
        alcance: Alcance,
        ahora: Instante,
    ) -> Result<Self, ErrorDominio> {
        if usuario.trim().is_empty() {
            return Err(ErrorDominio::Invalido(
                "el nombre de usuario es obligatorio".into(),
            ));
        }
        Ok(Usuario {
            id: Uuid::new_v4(),
            usuario: usuario.trim().to_string(),
            credencial,
            roles,
            alcance,
            activo: true,
            actualizado: ahora,
        })
    }
}

/// Permisos efectivos = unión de los permisos de los roles activos del usuario.
#[must_use]
pub fn permisos_efectivos(usuario: &Usuario, roles: &[Rol]) -> BTreeSet<Permiso> {
    roles
        .iter()
        .filter(|r| r.activo && usuario.roles.contains(&r.id))
        .flat_map(|r| r.permisos.iter().copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credencial_hashea_y_verifica() {
        let c = Credencial::nueva("secreta123").unwrap();
        assert_ne!(c.hash(), "secreta123"); // nunca texto plano
        assert!(c.verificar("secreta123"));
        assert!(!c.verificar("otra"));
    }

    #[test]
    fn permisos_efectivos_son_union_de_roles() {
        let ahora = Instante::ahora();
        let cajero =
            Rol::nuevo("cajero", [Permiso::Vender, Permiso::RegistrarGasto], ahora).unwrap();
        let precios = Rol::nuevo("precios", [Permiso::EditarPrecio], ahora).unwrap();
        let u = Usuario::nuevo(
            "ana",
            Credencial::nueva("clave").unwrap(),
            vec![cajero.id, precios.id],
            Alcance::Todas,
            ahora,
        )
        .unwrap();
        let efectivos = permisos_efectivos(&u, &[cajero, precios]);
        assert!(efectivos.contains(&Permiso::Vender));
        assert!(efectivos.contains(&Permiso::EditarPrecio));
        assert!(efectivos.contains(&Permiso::RegistrarGasto));
    }
}
