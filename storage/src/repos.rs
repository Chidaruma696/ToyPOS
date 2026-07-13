//! Repositorios como **traits agnósticos de almacenamiento**: el dominio y las
//! caras futuras dependen de estas interfaces, no de SQLite (D1/D7). Toda
//! operación recibe el `ContextoAcceso` que aplica permiso, alcance y auditoría.

use std::collections::BTreeSet;

use domain::acceso::{ContextoAcceso, Permiso};
use domain::aprobacion::Aprobacion;
use domain::barcode::Ean13;
use domain::caja::{Corte, ResumenVentas, SesionCaja};
use domain::envio::Envio;
use domain::etiqueta::{CajaEtiquetado, Etiqueta};
use domain::gasto::Gasto;
use domain::inventario::{Cantidad, MovimientoInventario};
use domain::notificacion::{Notificacion, TipoNotificacion};
use domain::precio::Nivel;
use domain::producto::{OrigenProducto, Producto, TipoProducto, VidaUtil};
use domain::sucursal::{CodigoSucursal, Sucursal, TipoSucursal};
use domain::tiempo::{Instante, ZonaHoraria};
use domain::unidades::{Centavos, Gramos};
use domain::usuario::{Rol, Usuario};
use uuid::Uuid;

use crate::error::Resultado;

/// Cómo se resuelve el código de barras al dar de alta un producto.
pub enum AltaCodigo {
    /// `peso_variable`: sin código de producto.
    Ninguno,
    /// `pieza` comprada: GTIN de fábrica.
    Externo(Ean13),
    /// `pieza` producida por matriz: el sistema acuña el EAN-13 (D22).
    GenerarMatriz,
}

/// Datos para dar de alta un producto en el catálogo global.
pub struct BorradorProducto {
    pub nombre: String,
    pub tipo: TipoProducto,
    pub origen: OrigenProducto,
    pub codigo: AltaCodigo,
    pub peso_empaque: Option<Gramos>,
    /// Vida útil en meses (D33); `None` = default de 9.
    pub vida_util: Option<VidaUtil>,
}

/// Una entrada de la bitácora de auditoría (D11).
#[derive(Debug, Clone)]
pub struct EntradaBitacora {
    pub actor: String,
    pub operacion: String,
    pub entidad: String,
    pub entidad_id: Uuid,
    pub antes: Option<String>,
    pub despues: Option<String>,
    pub ts: String,
}

/// Organización, identidad y acceso (`organization-access`).
#[allow(async_fn_in_trait)]
pub trait Organizacion {
    async fn crear_sucursal(
        &self,
        ctx: &ContextoAcceso,
        tipo: TipoSucursal,
        codigo: CodigoSucursal,
        nombre: &str,
        zona: ZonaHoraria,
    ) -> Resultado<Sucursal>;
    async fn sucursal_por_codigo(
        &self,
        ctx: &ContextoAcceso,
        codigo: &str,
    ) -> Resultado<Option<Sucursal>>;
    /// Renombra la sucursal; su **código es inmutable** y no se toca (D18).
    async fn renombrar_sucursal(
        &self,
        ctx: &ContextoAcceso,
        id: Uuid,
        nuevo_nombre: &str,
    ) -> Resultado<()>;
    async fn listar_sucursales(&self, ctx: &ContextoAcceso) -> Resultado<Vec<Sucursal>>;

    async fn crear_rol(
        &self,
        ctx: &ContextoAcceso,
        nombre: &str,
        permisos: &[Permiso],
    ) -> Resultado<Rol>;
    async fn agregar_permiso_a_rol(
        &self,
        ctx: &ContextoAcceso,
        rol: Uuid,
        permiso: Permiso,
    ) -> Resultado<()>;
    async fn quitar_permiso_a_rol(
        &self,
        ctx: &ContextoAcceso,
        rol: Uuid,
        permiso: Permiso,
    ) -> Resultado<()>;

    async fn crear_usuario(
        &self,
        ctx: &ContextoAcceso,
        usuario: &str,
        contrasena: &str,
        roles: &[Uuid],
        alcance: domain::acceso::Alcance,
    ) -> Resultado<Usuario>;
    /// Baja lógica: el usuario deja de autenticarse; historial y atribución se conservan.
    async fn desactivar_usuario(&self, ctx: &ContextoAcceso, id: Uuid) -> Resultado<()>;

    /// Login local (offline-ok): valida credenciales y arma el `ContextoAcceso`.
    async fn autenticar(&self, usuario: &str, contrasena: &str) -> Resultado<ContextoAcceso>;
}

/// Catálogo de productos (`product-catalog`).
#[allow(async_fn_in_trait)]
pub trait Catalogo {
    async fn crear_producto(
        &self,
        ctx: &ContextoAcceso,
        borrador: BorradorProducto,
    ) -> Resultado<Producto>;
    async fn producto_por_codigo(
        &self,
        ctx: &ContextoAcceso,
        codigo: &str,
    ) -> Resultado<Option<Producto>>;
    async fn desactivar_producto(&self, ctx: &ContextoAcceso, id: Uuid) -> Resultado<()>;
    /// Edita la vida útil (D33); los etiquetados futuros la usan, lo ya
    /// emitido conserva su caducidad. Requiere `gestionar_productos`.
    async fn fijar_vida_util(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        vida: VidaUtil,
    ) -> Resultado<()>;
}

/// Precios (`pricing`).
#[allow(async_fn_in_trait)]
pub trait Precios {
    async fn fijar_precio(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
        nivel: Nivel,
        monto: Centavos,
    ) -> Resultado<()>;
    async fn copiar_precios(
        &self,
        ctx: &ContextoAcceso,
        origen: Uuid,
        destino: Uuid,
        productos: &[Uuid],
    ) -> Resultado<u64>;
    async fn niveles_con_precio(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
    ) -> Resultado<BTreeSet<Nivel>>;
    async fn esta_congelado(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
    ) -> Resultado<bool>;
    async fn vendible(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
        nivel: Nivel,
    ) -> Resultado<bool>;
}

/// Gastos y corte de caja (`cash-management` + `authorization-workflow`).
#[allow(async_fn_in_trait)]
pub trait Caja {
    async fn abrir_sesion(
        &self,
        ctx: &ContextoAcceso,
        sucursal: Uuid,
        fondo: Centavos,
    ) -> Resultado<SesionCaja>;
    async fn registrar_gasto(
        &self,
        ctx: &ContextoAcceso,
        sucursal: Uuid,
        sesion: Uuid,
        monto: Centavos,
        concepto: &str,
    ) -> Resultado<Gasto>;
    async fn aprobar_gasto(&self, ctx: &ContextoAcceso, gasto: Uuid) -> Resultado<()>;
    async fn rechazar_gasto(
        &self,
        ctx: &ContextoAcceso,
        gasto: Uuid,
        motivo: &str,
    ) -> Resultado<()>;
    async fn cancelar_gasto(
        &self,
        ctx: &ContextoAcceso,
        gasto: Uuid,
        motivo: &str,
    ) -> Resultado<()>;
    async fn aprobacion_de(&self, ctx: &ContextoAcceso, gasto: Uuid) -> Resultado<Aprobacion>;
    /// Las aprobaciones pendientes dirigidas a este aprobador (su alcance);
    /// requiere `autorizar_gasto`.
    async fn aprobaciones_pendientes(&self, ctx: &ContextoAcceso) -> Resultado<Vec<Aprobacion>>;
    async fn listar_gastos(&self, ctx: &ContextoAcceso, sucursal: Uuid) -> Resultado<Vec<Gasto>>;
    async fn cerrar_sesion(
        &self,
        ctx: &ContextoAcceso,
        sesion: Uuid,
        contado: Centavos,
        ventas: ResumenVentas,
    ) -> Resultado<()>;
    /// Revela el corte (esperado/diferencia): requiere `VerConciliacion`.
    async fn conciliacion(&self, ctx: &ContextoAcceso, sesion: Uuid) -> Resultado<Corte>;
}

/// Inventario por sucursal (`inventory`).
#[allow(async_fn_in_trait)]
pub trait Inventario {
    /// Ajuste **absoluto** (D26): fija la existencia a la cantidad contada
    /// `objetivo`, con `motivo`. Exige `ajustar_inventario` + alcance.
    async fn ajustar(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
        objetivo: Cantidad,
        motivo: &str,
    ) -> Resultado<()>;
    /// Revierte o rehace un movimiento (toggle `aplicado ⇄ revertido`, D27).
    async fn revertir_movimiento(&self, ctx: &ContextoAcceso, movimiento: Uuid) -> Resultado<()>;
    /// Existencia actual (lectura O(1) del saldo); exige `ver_inventario` + alcance.
    async fn existencia(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
    ) -> Resultado<Cantidad>;
    /// Historial de movimientos del producto en la sucursal (dentro del alcance).
    async fn movimientos_de(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
    ) -> Resultado<Vec<MovimientoInventario>>;
}

/// Etiquetado de producción (`labeling`). No toca existencias: matriz produce
/// sin llevar stock propio (D34).
#[allow(async_fn_in_trait)]
pub trait Etiquetado {
    /// Etiqueta una serie de pesadas de un `peso_variable`: una etiqueta
    /// por-ítem por pesada, con discriminador antiduplicado (D31). Exige
    /// `etiquetar` + alcance.
    async fn etiquetar_pesadas(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
        pesos: &[Gramos],
    ) -> Resultado<Vec<Etiqueta>>;
    /// Cierra una caja que agrupa etiquetas ya emitidas (mismo producto y
    /// sucursal, activas y sin caja previa, D35).
    async fn cerrar_caja_pesadas(
        &self,
        ctx: &ContextoAcceso,
        etiquetas: &[Uuid],
    ) -> Resultado<CajaEtiquetado>;
    /// Cierra una caja de un `pieza` con la cantidad contenida (D35).
    async fn cerrar_caja_pieza(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
        cantidad: i64,
    ) -> Resultado<CajaEtiquetado>;
    /// Lookup local del barcode per-ítem (D31); `None` fuera del alcance.
    async fn etiqueta_por_codigo(
        &self,
        ctx: &ContextoAcceso,
        codigo: &str,
    ) -> Resultado<Option<Etiqueta>>;
    /// Las etiquetas agrupadas en una caja (N y peso total consultables).
    async fn etiquetas_de_caja(&self, ctx: &ContextoAcceso, caja: Uuid)
    -> Resultado<Vec<Etiqueta>>;
    /// Barrido "por vencer" (D36): etiquetas y cajas activas en expendios a
    /// ≤ 5 días de caducar (día en la zona de cada sucursal respecto a
    /// `referencia`), agrupadas por producto × sucursal, notificadas a la
    /// sucursal y a la matriz. Idempotente; **solo el actor `sistema`**.
    /// Devuelve cuántas notificaciones emitió.
    async fn barrer_por_vencer(&self, ctx: &ContextoAcceso, referencia: Instante)
    -> Resultado<u64>;
}

/// El contenido de un envío: sus cajas y sus etiquetas **sueltas** (las
/// agrupadas en una caja viajan con ella y se consultan vía `etiquetas_de_caja`).
/// Tras recibir, presentes y faltantes se distinguen por su estado.
#[derive(Debug, Clone)]
pub struct ContenidoEnvio {
    pub cajas: Vec<CajaEtiquetado>,
    pub etiquetas_sueltas: Vec<Etiqueta>,
}

/// Envíos entre sucursales (`shipments` · D38–D43).
#[allow(async_fn_in_trait)]
pub trait Envios {
    /// Prepara un envío ligando su contenido (cajas y etiquetas sueltas del
    /// origen, activas y libres, D42). Exige `enviar` + alcance sobre el origen;
    /// un extremo debe ser la matriz (D39). Asigna folio (D18).
    async fn preparar_envio(
        &self,
        ctx: &ContextoAcceso,
        origen: Uuid,
        destino: Uuid,
        cajas: &[Uuid],
        etiquetas_sueltas: &[Uuid],
    ) -> Resultado<Envio>;
    /// Marca el envío como enviado. En una devolución (origen expendio) postea
    /// la salida `envio` por producto, sujeta a no-negatividad (D40/D25).
    async fn marcar_enviado(&self, ctx: &ContextoAcceso, envio: Uuid) -> Resultado<()>;
    /// Cancela un envío preparado y libera su contenido (D42).
    async fn cancelar_envio(&self, ctx: &ContextoAcceso, envio: Uuid) -> Resultado<()>;
    /// Recibe **lo real**: lo presente cambia al destino (y postea `recepcion`
    /// por producto si el destino es expendio); lo faltante queda extraviado,
    /// anotado en el documento y notificado a la administración (D40/D41).
    async fn recibir_envio(
        &self,
        ctx: &ContextoAcceso,
        envio: Uuid,
        presentes: &[Uuid],
    ) -> Resultado<Envio>;
    /// El envío por folio, con su contenido; dentro del alcance de alguno de
    /// sus extremos.
    async fn envio_por_folio(
        &self,
        ctx: &ContextoAcceso,
        folio: &str,
    ) -> Resultado<Option<(Envio, ContenidoEnvio)>>;
}

/// Notificaciones del sistema a usuarios (`notifications` · D37).
#[allow(async_fn_in_trait)]
pub trait Notificaciones {
    /// Emite una notificación a la bandeja de una sucursal (solo el `sistema`).
    async fn emitir(
        &self,
        ctx: &ContextoAcceso,
        tipo: TipoNotificacion,
        mensaje: &str,
        sucursal: Uuid,
    ) -> Resultado<Notificacion>;
    /// La bandeja de la sucursal; exige `ver_notificaciones` + alcance.
    async fn bandeja(&self, ctx: &ContextoAcceso, sucursal: Uuid) -> Resultado<Vec<Notificacion>>;
    /// Marca leída a nivel sucursal (idempotente, conserva la fila, D37).
    async fn marcar_leida(&self, ctx: &ContextoAcceso, notificacion: Uuid) -> Resultado<()>;
}

/// Auditoría transversal (`organization-access` · D11).
#[allow(async_fn_in_trait)]
pub trait Auditoria {
    async fn bitacora_de(
        &self,
        ctx: &ContextoAcceso,
        entidad: &str,
        id: Uuid,
    ) -> Resultado<Vec<EntradaBitacora>>;
}
