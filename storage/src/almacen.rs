//! Backend SQLite de los repositorios. Punto **único** de acceso a datos: aquí
//! se aplican el aislamiento por alcance (D3) y la bitácora de auditoría (D11),
//! y viven los generadores de folio (D18) y de barcode de matriz (D22).

use std::collections::BTreeSet;
use std::fmt::Display;
use std::str::FromStr;

use domain::acceso::{Actor, Alcance, ContextoAcceso, Permiso};
use domain::aprobacion::{Aprobacion, EstadoAprobacion};
use domain::barcode::{Ean13, MODULO_DISCRIMINADOR, PREFIJO_MATRIZ};
use domain::caja::{Corte, EstadoSesion, ResumenVentas, SesionCaja};
use domain::error::ErrorDominio;
use domain::etiqueta::{CajaEtiquetado, EstadoEtiqueta, Etiqueta};
use domain::folio::{Folio, TipoDocumento};
use domain::gasto::Gasto;
use domain::inventario::{
    Cantidad, EstadoMovimiento, Existencia, MovimientoInventario, TipoMovimiento,
};
use domain::notificacion::{Notificacion, TipoNotificacion};
use domain::precio::{self, Nivel};
use domain::producto::{
    CodigoBarras, OrigenProducto, Producto, TipoProducto, UnidadVenta, VidaUtil,
};
use domain::sucursal::{CodigoSucursal, Sucursal, TipoSucursal};
use domain::tiempo::{Instante, ZonaHoraria};
use domain::unidades::{Centavos, Gramos};
use domain::usuario::{Credencial, Rol, Usuario};
use jiff::Span;
use jiff::civil::Date;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::{ErrorAlmacen, Resultado};
use crate::migraciones;
use crate::repos::{
    AltaCodigo, Auditoria, BorradorProducto, Caja, Catalogo, EntradaBitacora, Etiquetado,
    Inventario, Notificaciones, Organizacion, Precios,
};

/// Todos los permisos: se otorgan al administrador inicial en el bootstrap.
const TODOS_LOS_PERMISOS: [Permiso; 15] = [
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

/// Días de anticipación de la alerta "por vencer" (D36).
const DIAS_ALERTA_POR_VENCER: i64 = 5;

/// Almacén local: envuelve el pool de SQLite. Clonar comparte el pool (Arc).
#[derive(Clone)]
pub struct Almacen {
    pool: SqlitePool,
}

impl Almacen {
    /// Abre un almacén contra la URL de SQLite dada, creándolo si falta y
    /// aplicando el esquema.
    pub async fn abrir(url: &str) -> Resultado<Self> {
        let opts = SqliteConnectOptions::from_str(url)
            .map_err(ErrorAlmacen::Sqlx)?
            .create_if_missing(true)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_secs(5));
        // Un único escritor: SQLite serializa las escrituras de todos modos, y una
        // sola conexión da aislamiento perfecto sin el deadlock lectura→escritura
        // del journal por reversión. La responsividad la sostiene la capa async
        // (Tokio, D14/D17); cada operación local es sub-ms.
        let pool = SqlitePoolOptions::new()
            .min_connections(1)
            .max_connections(1)
            .connect_with(opts)
            .await?;
        let alm = Almacen { pool };
        alm.aplicar_esquema().await?;
        Ok(alm)
    }

    async fn aplicar_esquema(&self) -> Resultado<()> {
        sqlx::raw_sql(migraciones::ESQUEMA)
            .execute(&self.pool)
            .await?;
        for columna in migraciones::COLUMNAS_ADITIVAS {
            if let Err(e) = sqlx::query(columna).execute(&self.pool).await {
                let ya_existe = matches!(&e, sqlx::Error::Database(db)
                    if db.message().contains("duplicate column"));
                if !ya_existe {
                    return Err(e.into());
                }
            }
        }
        for trigger in migraciones::TRIGGERS {
            sqlx::query(trigger).execute(&self.pool).await?;
        }
        Ok(())
    }

    /// Arranque de un sistema vacío: crea la matriz y un administrador inicial
    /// (alcance total + todos los permisos), atribuidos al actor `sistema` (D3.10).
    pub async fn bootstrap(
        &self,
        codigo_matriz: &str,
        admin: &str,
        contrasena: &str,
    ) -> Resultado<(Sucursal, Usuario)> {
        let sistema = ContextoAcceso::sistema();
        let existentes = self.listar_sucursales(&sistema).await?;
        if !existentes.is_empty() {
            return Err(ErrorAlmacen::Conflicto(
                "el sistema ya está inicializado".into(),
            ));
        }
        let matriz = self
            .crear_sucursal(
                &sistema,
                TipoSucursal::Matriz,
                CodigoSucursal::nueva(codigo_matriz)?,
                "Matriz",
                ZonaHoraria::default(),
            )
            .await?;
        let rol = self
            .crear_rol(&sistema, "administrador", &TODOS_LOS_PERMISOS)
            .await?;
        let usuario = self
            .crear_usuario(&sistema, admin, contrasena, &[rol.id], Alcance::Todas)
            .await?;
        Ok((matriz, usuario))
    }

    // ---- helpers internos ----

    async fn siguiente_barcode(conn: &mut sqlx::SqliteConnection) -> Resultado<u64> {
        let row = sqlx::query(
            "INSERT INTO barcode_seq (id, siguiente) VALUES (1, 1) \
             ON CONFLICT(id) DO UPDATE SET siguiente = siguiente + 1 RETURNING siguiente",
        )
        .fetch_one(conn)
        .await?;
        let n: i64 = row.try_get("siguiente")?;
        Ok(n as u64)
    }

    async fn siguiente_folio(
        conn: &mut sqlx::SqliteConnection,
        sucursal: Uuid,
        codigo: &CodigoSucursal,
        tipo: TipoDocumento,
    ) -> Resultado<Folio> {
        let row = sqlx::query(
            "INSERT INTO folio_seq (sucursal_id, tipo, siguiente) VALUES (?, ?, 1) \
             ON CONFLICT(sucursal_id, tipo) DO UPDATE SET siguiente = siguiente + 1 RETURNING siguiente",
        )
        .bind(sucursal.to_string())
        .bind(tipo.letra().to_string())
        .fetch_one(conn)
        .await?;
        let consecutivo: i64 = row.try_get("siguiente")?;
        Ok(Folio::componer(codigo, tipo, consecutivo as u64))
    }

    async fn codigo_de_sucursal(&self, sucursal: Uuid) -> Resultado<CodigoSucursal> {
        let row = sqlx::query("SELECT codigo FROM sucursal WHERE id = ?")
            .bind(sucursal.to_string())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("sucursal {sucursal}")))?;
        let codigo: String = row.try_get("codigo")?;
        CodigoSucursal::nueva(&codigo).map_err(Into::into)
    }

    /// Edición **en caliente** de los permisos de un rol (alta o baja de un
    /// permiso): el cambio aplica a quienes ya portan el rol al rearmar su
    /// contexto en el siguiente login (D2). Centraliza agregar/quitar (DRY).
    async fn editar_permisos_rol(
        &self,
        ctx: &ContextoAcceso,
        rol: Uuid,
        editar: impl FnOnce(&mut BTreeSet<Permiso>),
    ) -> Resultado<()> {
        ctx.requiere(Permiso::GestionarRoles)?;
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT permisos FROM rol WHERE id = ? AND activo = 1")
            .bind(rol.to_string())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("rol {rol}")))?;
        let antes: String = row.try_get("permisos")?;
        let mut permisos: BTreeSet<Permiso> = serde_json::from_str(&antes).map_err(corrupto)?;
        editar(&mut permisos);
        let despues = serde_json::to_string(&permisos).map_err(corrupto)?;
        sqlx::query("UPDATE rol SET permisos = ?, updated_at = ? WHERE id = ?")
            .bind(&despues)
            .bind(inst_txt(ahora))
            .bind(rol.to_string())
            .execute(&mut *tx)
            .await?;
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Modificar",
            "rol",
            rol,
            Some(antes),
            Some(despues),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

// ================== helpers libres ==================

fn corrupto<E: Display>(e: E) -> ErrorAlmacen {
    ErrorAlmacen::Corrupto(e.to_string())
}

fn mapear_conflicto(e: sqlx::Error) -> ErrorAlmacen {
    if let sqlx::Error::Database(ref db) = e {
        if db.is_unique_violation() {
            return ErrorAlmacen::Conflicto(db.message().to_string());
        }
    }
    ErrorAlmacen::Sqlx(e)
}

fn exigir_alcance(ctx: &ContextoAcceso, sucursal: Uuid) -> Resultado<()> {
    if ctx.alcance.cubre(sucursal) {
        Ok(())
    } else {
        Err(ErrorDominio::FueraDeAlcance.into())
    }
}

fn inst_txt(i: Instante) -> String {
    i.to_string()
}

fn inst_de(s: &str) -> Resultado<Instante> {
    let ts = jiff::Timestamp::from_str(s).map_err(corrupto)?;
    Ok(Instante::desde_timestamp(ts))
}

fn uuid_de(s: &str) -> Resultado<Uuid> {
    Uuid::parse_str(s).map_err(corrupto)
}

fn alcance_json(a: &Alcance) -> String {
    serde_json::to_string(a).unwrap_or_else(|_| "\"Todas\"".into())
}

fn tipo_suc_txt(t: TipoSucursal) -> &'static str {
    match t {
        TipoSucursal::Matriz => "Matriz",
        TipoSucursal::Expendio => "Expendio",
    }
}

fn tipo_suc_de(s: &str) -> Resultado<TipoSucursal> {
    match s {
        "Matriz" => Ok(TipoSucursal::Matriz),
        "Expendio" => Ok(TipoSucursal::Expendio),
        otro => Err(corrupto(format!("tipo de sucursal desconocido: {otro}"))),
    }
}

fn tipo_prod_txt(t: TipoProducto) -> &'static str {
    match t {
        TipoProducto::PesoVariable => "PesoVariable",
        TipoProducto::Pieza => "Pieza",
    }
}

fn tipo_prod_de(s: &str) -> Resultado<TipoProducto> {
    match s {
        "PesoVariable" => Ok(TipoProducto::PesoVariable),
        "Pieza" => Ok(TipoProducto::Pieza),
        otro => Err(corrupto(format!("tipo de producto desconocido: {otro}"))),
    }
}

fn origen_txt(o: OrigenProducto) -> &'static str {
    match o {
        OrigenProducto::Matriz => "Matriz",
        OrigenProducto::Externo => "Externo",
    }
}

fn origen_de(s: &str) -> Resultado<OrigenProducto> {
    match s {
        "Matriz" => Ok(OrigenProducto::Matriz),
        "Externo" => Ok(OrigenProducto::Externo),
        otro => Err(corrupto(format!("origen desconocido: {otro}"))),
    }
}

fn nivel_txt(n: Nivel) -> &'static str {
    match n {
        Nivel::Menudeo => "Menudeo",
        Nivel::MedioMayoreo => "MedioMayoreo",
        Nivel::Mayoreo => "Mayoreo",
    }
}

fn nivel_de(s: &str) -> Resultado<Nivel> {
    match s {
        "Menudeo" => Ok(Nivel::Menudeo),
        "MedioMayoreo" => Ok(Nivel::MedioMayoreo),
        "Mayoreo" => Ok(Nivel::Mayoreo),
        otro => Err(corrupto(format!("nivel desconocido: {otro}"))),
    }
}

fn estado_aprob_txt(e: EstadoAprobacion) -> &'static str {
    match e {
        EstadoAprobacion::Pendiente => "Pendiente",
        EstadoAprobacion::Aprobada => "Aprobada",
        EstadoAprobacion::Rechazada => "Rechazada",
        EstadoAprobacion::Cancelada => "Cancelada",
    }
}

fn estado_aprob_de(s: &str) -> Resultado<EstadoAprobacion> {
    match s {
        "Pendiente" => Ok(EstadoAprobacion::Pendiente),
        "Aprobada" => Ok(EstadoAprobacion::Aprobada),
        "Rechazada" => Ok(EstadoAprobacion::Rechazada),
        "Cancelada" => Ok(EstadoAprobacion::Cancelada),
        otro => Err(corrupto(format!(
            "estado de aprobación desconocido: {otro}"
        ))),
    }
}

fn tipo_mov_txt(t: TipoMovimiento) -> &'static str {
    match t {
        TipoMovimiento::Ajuste => "Ajuste",
        TipoMovimiento::Recepcion => "Recepcion",
        TipoMovimiento::Venta => "Venta",
    }
}

fn tipo_mov_de(s: &str) -> Resultado<TipoMovimiento> {
    match s {
        "Ajuste" => Ok(TipoMovimiento::Ajuste),
        "Recepcion" => Ok(TipoMovimiento::Recepcion),
        "Venta" => Ok(TipoMovimiento::Venta),
        otro => Err(corrupto(format!("tipo de movimiento desconocido: {otro}"))),
    }
}

fn estado_mov_txt(e: EstadoMovimiento) -> &'static str {
    match e {
        EstadoMovimiento::Aplicado => "Aplicado",
        EstadoMovimiento::Revertido => "Revertido",
    }
}

fn estado_mov_de(s: &str) -> Resultado<EstadoMovimiento> {
    match s {
        "Aplicado" => Ok(EstadoMovimiento::Aplicado),
        "Revertido" => Ok(EstadoMovimiento::Revertido),
        otro => Err(corrupto(format!(
            "estado de movimiento desconocido: {otro}"
        ))),
    }
}

fn estado_eti_txt(e: EstadoEtiqueta) -> &'static str {
    match e {
        EstadoEtiqueta::Activa => "Activa",
        EstadoEtiqueta::Vendida => "Vendida",
    }
}

fn estado_eti_de(s: &str) -> Resultado<EstadoEtiqueta> {
    match s {
        "Activa" => Ok(EstadoEtiqueta::Activa),
        "Vendida" => Ok(EstadoEtiqueta::Vendida),
        otro => Err(corrupto(format!("estado de etiqueta desconocido: {otro}"))),
    }
}

fn tipo_notif_txt(t: TipoNotificacion) -> &'static str {
    match t {
        TipoNotificacion::PorVencer => "PorVencer",
    }
}

fn tipo_notif_de(s: &str) -> Resultado<TipoNotificacion> {
    match s {
        "PorVencer" => Ok(TipoNotificacion::PorVencer),
        otro => Err(corrupto(format!(
            "tipo de notificación desconocido: {otro}"
        ))),
    }
}

fn fecha_de(s: &str) -> Resultado<Date> {
    s.parse().map_err(corrupto)
}

fn map_etiqueta(r: &SqliteRow) -> Resultado<Etiqueta> {
    let caja: Option<String> = r.try_get("caja_id")?;
    Ok(Etiqueta {
        id: uuid_de(r.try_get("id")?)?,
        codigo: Ean13::parse(r.try_get("codigo")?)?,
        discriminador: r.try_get::<i64, _>("discriminador")? as u64,
        producto: uuid_de(r.try_get("producto_id")?)?,
        peso: Gramos::new(r.try_get("peso")?),
        sucursal: uuid_de(r.try_get("sucursal_id")?)?,
        caja: caja.map(|c| uuid_de(&c)).transpose()?,
        fecha_etiquetado: inst_de(r.try_get("fecha_etiquetado")?)?,
        caducidad: fecha_de(r.try_get("caducidad")?)?,
        estado: estado_eti_de(r.try_get("estado")?)?,
        actualizado: inst_de(r.try_get("updated_at")?)?,
    })
}

fn map_notificacion(r: &SqliteRow) -> Resultado<Notificacion> {
    let leida: Option<String> = r.try_get("leida_en")?;
    Ok(Notificacion {
        id: uuid_de(r.try_get("id")?)?,
        tipo: tipo_notif_de(r.try_get("tipo")?)?,
        mensaje: r.try_get("mensaje")?,
        sucursal: uuid_de(r.try_get("sucursal_id")?)?,
        creado: inst_de(r.try_get("creado")?)?,
        leida_en: leida.map(|l| inst_de(&l)).transpose()?,
        actualizado: inst_de(r.try_get("updated_at")?)?,
    })
}

/// Zona horaria de una sucursal, con la conexión de la transacción.
async fn zona_tx(conn: &mut sqlx::SqliteConnection, sucursal: Uuid) -> Resultado<ZonaHoraria> {
    let row = sqlx::query("SELECT zona FROM sucursal WHERE id = ?")
        .bind(sucursal.to_string())
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("sucursal {sucursal}")))?;
    ZonaHoraria::nueva(row.try_get("zona")?).map_err(Into::into)
}

async fn insertar_notificacion(
    conn: &mut sqlx::SqliteConnection,
    n: &Notificacion,
    grupo: Option<&str>,
) -> Resultado<()> {
    sqlx::query(
        "INSERT INTO notificacion (id, tipo, mensaje, sucursal_id, grupo, creado, leida_en, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, NULL, ?)",
    )
    .bind(n.id.to_string())
    .bind(tipo_notif_txt(n.tipo))
    .bind(&n.mensaje)
    .bind(n.sucursal.to_string())
    .bind(grupo)
    .bind(inst_txt(n.creado))
    .bind(inst_txt(n.actualizado))
    .execute(conn)
    .await?;
    Ok(())
}

fn map_movimiento(r: &SqliteRow, unidad: UnidadVenta) -> Resultado<MovimientoInventario> {
    Ok(MovimientoInventario {
        id: uuid_de(r.try_get("id")?)?,
        producto: uuid_de(r.try_get("producto_id")?)?,
        sucursal: uuid_de(r.try_get("sucursal_id")?)?,
        tipo: tipo_mov_de(r.try_get("tipo")?)?,
        delta: Cantidad::en(unidad, r.try_get("delta")?),
        motivo: r.try_get("motivo")?,
        estado: estado_mov_de(r.try_get("estado")?)?,
        registrado: inst_de(r.try_get("registrado")?)?,
        actualizado: inst_de(r.try_get("updated_at")?)?,
    })
}

/// Producto por id usando la conexión de una transacción (para no pedir otra al pool).
async fn producto_tx(conn: &mut sqlx::SqliteConnection, id: Uuid) -> Resultado<Producto> {
    let row = sqlx::query("SELECT * FROM producto WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("producto {id}")))?;
    map_producto(&row)
}

/// Saldo materializado de la existencia; cero si aún no hay fila (perezosa, D30).
async fn saldo_tx(
    conn: &mut sqlx::SqliteConnection,
    producto: Uuid,
    sucursal: Uuid,
    unidad: UnidadVenta,
) -> Resultado<Cantidad> {
    let row =
        sqlx::query("SELECT cantidad FROM existencia WHERE producto_id = ? AND sucursal_id = ?")
            .bind(producto.to_string())
            .bind(sucursal.to_string())
            .fetch_optional(conn)
            .await?;
    Ok(match row {
        Some(r) => Cantidad::en(unidad, r.try_get("cantidad")?),
        None => Cantidad::cero(unidad),
    })
}

/// Fija el saldo materializado (crea la fila si no existía, materialización perezosa D30).
async fn fijar_existencia(
    conn: &mut sqlx::SqliteConnection,
    producto: Uuid,
    sucursal: Uuid,
    cantidad: i64,
    ahora: Instante,
) -> Resultado<()> {
    sqlx::query(
        "INSERT INTO existencia (producto_id, sucursal_id, cantidad, updated_at) VALUES (?, ?, ?, ?) \
         ON CONFLICT(producto_id, sucursal_id) DO UPDATE SET cantidad = excluded.cantidad, updated_at = excluded.updated_at",
    )
    .bind(producto.to_string())
    .bind(sucursal.to_string())
    .bind(cantidad)
    .bind(inst_txt(ahora))
    .execute(conn)
    .await?;
    Ok(())
}

async fn insertar_movimiento(
    conn: &mut sqlx::SqliteConnection,
    m: &MovimientoInventario,
) -> Resultado<()> {
    sqlx::query(
        "INSERT INTO movimiento_inventario (id, producto_id, sucursal_id, tipo, delta, motivo, estado, registrado, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(m.id.to_string())
    .bind(m.producto.to_string())
    .bind(m.sucursal.to_string())
    .bind(tipo_mov_txt(m.tipo))
    .bind(m.delta.magnitud())
    .bind(m.motivo.clone())
    .bind(estado_mov_txt(m.estado))
    .bind(inst_txt(m.registrado))
    .bind(inst_txt(m.actualizado))
    .execute(conn)
    .await?;
    Ok(())
}

fn map_sucursal(r: &SqliteRow) -> Resultado<Sucursal> {
    Ok(Sucursal {
        id: uuid_de(r.try_get("id")?)?,
        tipo: tipo_suc_de(r.try_get("tipo")?)?,
        codigo: CodigoSucursal::nueva(r.try_get("codigo")?)?,
        nombre: r.try_get("nombre")?,
        zona: ZonaHoraria::nueva(r.try_get("zona")?)?,
        activo: r.try_get::<i64, _>("activo")? != 0,
        actualizado: inst_de(r.try_get("updated_at")?)?,
    })
}

fn map_producto(r: &SqliteRow) -> Resultado<Producto> {
    let origen = origen_de(r.try_get("origen")?)?;
    let codigo_txt: Option<String> = r.try_get("codigo")?;
    let codigo = match codigo_txt {
        None => None,
        Some(t) => {
            let ean = Ean13::parse(&t)?;
            Some(match origen {
                OrigenProducto::Externo => CodigoBarras::Externo(ean),
                OrigenProducto::Matriz => CodigoBarras::Matriz(ean),
            })
        }
    };
    let peso: Option<i64> = r.try_get("peso_empaque")?;
    Ok(Producto {
        id: uuid_de(r.try_get("id")?)?,
        nombre: r.try_get("nombre")?,
        tipo: tipo_prod_de(r.try_get("tipo")?)?,
        origen,
        codigo,
        peso_empaque: peso.map(Gramos::new),
        vida_util: VidaUtil::nueva(r.try_get("vida_util")?)?,
        activo: r.try_get::<i64, _>("activo")? != 0,
        actualizado: inst_de(r.try_get("updated_at")?)?,
    })
}

fn map_gasto(r: &SqliteRow) -> Resultado<Gasto> {
    Ok(Gasto {
        id: uuid_de(r.try_get("id")?)?,
        folio: Folio::desde_texto(r.try_get("folio")?),
        monto: Centavos::new(r.try_get("monto")?),
        concepto: r.try_get("concepto")?,
        cajero: uuid_de(r.try_get("cajero_id")?)?,
        sucursal: uuid_de(r.try_get("sucursal_id")?)?,
        sesion: uuid_de(r.try_get("sesion_id")?)?,
        registrado: inst_de(r.try_get("registrado")?)?,
        actualizado: inst_de(r.try_get("updated_at")?)?,
    })
}

fn map_aprobacion(r: &SqliteRow) -> Resultado<Aprobacion> {
    let resolutor: Option<String> = r.try_get("resolutor_id")?;
    let resuelto: Option<String> = r.try_get("resuelto_en")?;
    Ok(Aprobacion {
        id: uuid_de(r.try_get("id")?)?,
        gasto: uuid_de(r.try_get("gasto_id")?)?,
        sucursal: uuid_de(r.try_get("sucursal_id")?)?,
        solicitante: uuid_de(r.try_get("solicitante_id")?)?,
        estado: estado_aprob_de(r.try_get("estado")?)?,
        resolutor: resolutor.map(|s| uuid_de(&s)).transpose()?,
        resuelto_en: resuelto.map(|s| inst_de(&s)).transpose()?,
        motivo: r.try_get("motivo")?,
        actualizado: inst_de(r.try_get("updated_at")?)?,
    })
}

// ================== impl Organizacion ==================

impl Organizacion for Almacen {
    async fn crear_sucursal(
        &self,
        ctx: &ContextoAcceso,
        tipo: TipoSucursal,
        codigo: CodigoSucursal,
        nombre: &str,
        zona: ZonaHoraria,
    ) -> Resultado<Sucursal> {
        ctx.requiere(Permiso::GestionarSucursales)?;
        let ahora = Instante::ahora();
        let s = Sucursal::nueva(tipo, codigo, nombre, zona, ahora)?;

        let mut tx = self.pool.begin().await?;
        if tipo == TipoSucursal::Matriz {
            let n: i64 = sqlx::query("SELECT COUNT(*) AS n FROM sucursal WHERE tipo = 'Matriz'")
                .fetch_one(&mut *tx)
                .await?
                .try_get("n")?;
            if n > 0 {
                return Err(ErrorAlmacen::Conflicto(
                    "ya existe una sucursal matriz".into(),
                ));
            }
        }
        sqlx::query(
            "INSERT INTO sucursal (id, tipo, codigo, nombre, zona, activo, updated_at) VALUES (?, ?, ?, ?, ?, 1, ?)",
        )
        .bind(s.id.to_string())
        .bind(tipo_suc_txt(s.tipo))
        .bind(s.codigo.get())
        .bind(&s.nombre)
        .bind(s.zona.iana())
        .bind(inst_txt(s.actualizado))
        .execute(&mut *tx)
        .await
        .map_err(mapear_conflicto)?;
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Crear",
            "sucursal",
            s.id,
            None,
            Some(format!("{} {}", tipo_suc_txt(s.tipo), s.codigo.get())),
        )
        .await?;
        tx.commit().await?;
        Ok(s)
    }

    async fn sucursal_por_codigo(
        &self,
        ctx: &ContextoAcceso,
        codigo: &str,
    ) -> Resultado<Option<Sucursal>> {
        let row = sqlx::query("SELECT * FROM sucursal WHERE codigo = ?")
            .bind(codigo.to_uppercase())
            .fetch_optional(&self.pool)
            .await?;
        match row {
            None => Ok(None),
            Some(r) => {
                let s = map_sucursal(&r)?;
                Ok(if ctx.alcance.cubre(s.id) {
                    Some(s)
                } else {
                    None
                })
            }
        }
    }

    /// Renombra la sucursal. El **código no se toca**: es inmutable (D18), así
    /// que los folios históricos y futuros conservan su prefijo.
    async fn renombrar_sucursal(
        &self,
        ctx: &ContextoAcceso,
        id: Uuid,
        nuevo_nombre: &str,
    ) -> Resultado<()> {
        ctx.requiere(Permiso::GestionarSucursales)?;
        let nombre = nuevo_nombre.trim();
        if nombre.is_empty() {
            return Err(
                ErrorDominio::Invalido("el nombre de la sucursal es obligatorio".into()).into(),
            );
        }
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;
        let antes: String = sqlx::query("SELECT nombre FROM sucursal WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("sucursal {id}")))?
            .try_get("nombre")?;
        sqlx::query("UPDATE sucursal SET nombre = ?, updated_at = ? WHERE id = ?")
            .bind(nombre)
            .bind(inst_txt(ahora))
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Modificar",
            "sucursal",
            id,
            Some(antes),
            Some(nombre.to_string()),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn listar_sucursales(&self, ctx: &ContextoAcceso) -> Resultado<Vec<Sucursal>> {
        let rows = sqlx::query("SELECT * FROM sucursal ORDER BY codigo")
            .fetch_all(&self.pool)
            .await?;
        let mut out = Vec::new();
        for r in &rows {
            let s = map_sucursal(r)?;
            if ctx.alcance.cubre(s.id) {
                out.push(s);
            }
        }
        Ok(out)
    }

    async fn crear_rol(
        &self,
        ctx: &ContextoAcceso,
        nombre: &str,
        permisos: &[Permiso],
    ) -> Resultado<Rol> {
        ctx.requiere(Permiso::GestionarRoles)?;
        let ahora = Instante::ahora();
        let rol = Rol::nuevo(nombre, permisos.iter().copied(), ahora)?;
        let json = serde_json::to_string(&rol.permisos).map_err(corrupto)?;

        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO rol (id, nombre, permisos, activo, updated_at) VALUES (?, ?, ?, 1, ?)",
        )
        .bind(rol.id.to_string())
        .bind(&rol.nombre)
        .bind(&json)
        .bind(inst_txt(ahora))
        .execute(&mut *tx)
        .await?;
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Crear",
            "rol",
            rol.id,
            None,
            Some(json),
        )
        .await?;
        tx.commit().await?;
        Ok(rol)
    }

    async fn agregar_permiso_a_rol(
        &self,
        ctx: &ContextoAcceso,
        rol: Uuid,
        permiso: Permiso,
    ) -> Resultado<()> {
        self.editar_permisos_rol(ctx, rol, |p| {
            p.insert(permiso);
        })
        .await
    }

    async fn quitar_permiso_a_rol(
        &self,
        ctx: &ContextoAcceso,
        rol: Uuid,
        permiso: Permiso,
    ) -> Resultado<()> {
        self.editar_permisos_rol(ctx, rol, |p| {
            p.remove(&permiso);
        })
        .await
    }

    async fn crear_usuario(
        &self,
        ctx: &ContextoAcceso,
        usuario: &str,
        contrasena: &str,
        roles: &[Uuid],
        alcance: Alcance,
    ) -> Resultado<Usuario> {
        ctx.requiere(Permiso::GestionarUsuarios)?;
        let ahora = Instante::ahora();
        let cred = Credencial::nueva(contrasena)?;
        let u = Usuario::nuevo(usuario, cred, roles.to_vec(), alcance, ahora)?;
        let alcance_txt = alcance_json(&u.alcance);

        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO usuario (id, usuario, hash, alcance, activo, updated_at) VALUES (?, ?, ?, ?, 1, ?)")
            .bind(u.id.to_string())
            .bind(&u.usuario)
            .bind(u.credencial.hash())
            .bind(&alcance_txt)
            .bind(inst_txt(ahora))
            .execute(&mut *tx)
            .await
            .map_err(mapear_conflicto)?;
        for rol in &u.roles {
            sqlx::query("INSERT INTO usuario_rol (usuario_id, rol_id) VALUES (?, ?)")
                .bind(u.id.to_string())
                .bind(rol.to_string())
                .execute(&mut *tx)
                .await?;
        }
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Crear",
            "usuario",
            u.id,
            None,
            Some(u.usuario.clone()),
        )
        .await?;
        tx.commit().await?;
        Ok(u)
    }

    /// Baja **lógica** del usuario (D12/D16): deja de poder autenticarse, pero
    /// su historial y su atribución en la bitácora se conservan.
    async fn desactivar_usuario(&self, ctx: &ContextoAcceso, id: Uuid) -> Resultado<()> {
        ctx.requiere(Permiso::GestionarUsuarios)?;
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;
        let afectadas = sqlx::query(
            "UPDATE usuario SET activo = 0, updated_at = ? WHERE id = ? AND activo = 1",
        )
        .bind(inst_txt(ahora))
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if afectadas == 0 {
            return Err(ErrorAlmacen::NoEncontrado(format!("usuario activo {id}")));
        }
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Eliminar",
            "usuario",
            id,
            Some("activo".into()),
            Some("inactivo".into()),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn autenticar(&self, usuario: &str, contrasena: &str) -> Resultado<ContextoAcceso> {
        let credenciales_invalidas = || ErrorAlmacen::NoEncontrado("credenciales inválidas".into());
        let row =
            sqlx::query("SELECT id, hash, alcance FROM usuario WHERE usuario = ? AND activo = 1")
                .bind(usuario)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(credenciales_invalidas)?;
        let hash: String = row.try_get("hash")?;
        if !Credencial::desde_hash(hash).verificar(contrasena) {
            return Err(credenciales_invalidas());
        }
        let id = uuid_de(row.try_get("id")?)?;
        let alcance: Alcance = serde_json::from_str(row.try_get("alcance")?).map_err(corrupto)?;

        let filas = sqlx::query(
            "SELECT r.permisos FROM rol r JOIN usuario_rol ur ON ur.rol_id = r.id WHERE ur.usuario_id = ? AND r.activo = 1",
        )
        .bind(id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let mut permisos = BTreeSet::new();
        for f in &filas {
            let js: String = f.try_get("permisos")?;
            let v: Vec<Permiso> = serde_json::from_str(&js).map_err(corrupto)?;
            permisos.extend(v);
        }
        Ok(ContextoAcceso::nuevo(
            domain::acceso::Actor::Usuario(id),
            permisos,
            alcance,
        ))
    }
}

// ================== impl Catalogo ==================

impl Catalogo for Almacen {
    async fn crear_producto(
        &self,
        ctx: &ContextoAcceso,
        b: BorradorProducto,
    ) -> Resultado<Producto> {
        ctx.requiere(Permiso::GestionarProductos)?;
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;

        let codigo = match b.codigo {
            AltaCodigo::Ninguno => None,
            AltaCodigo::Externo(ean) => Some(CodigoBarras::Externo(ean)),
            AltaCodigo::GenerarMatriz => {
                let seq = Almacen::siguiente_barcode(&mut tx).await?;
                Some(CodigoBarras::Matriz(Ean13::generar_matriz(
                    PREFIJO_MATRIZ,
                    seq,
                )?))
            }
        };
        let mut p = Producto::nuevo(&b.nombre, b.tipo, b.origen, codigo, b.peso_empaque, ahora)?;
        if let Some(vida) = b.vida_util {
            p.vida_util = vida;
        }
        let codigo_txt = p.codigo.as_ref().map(|c| c.ean().get().to_string());
        let codigo_origen = p.codigo.as_ref().map(|c| match c {
            CodigoBarras::Externo(_) => "Externo",
            CodigoBarras::Matriz(_) => "Matriz",
        });

        sqlx::query(
            "INSERT INTO producto (id, nombre, tipo, origen, codigo, codigo_origen, peso_empaque, vida_util, activo, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?)",
        )
        .bind(p.id.to_string())
        .bind(&p.nombre)
        .bind(tipo_prod_txt(p.tipo))
        .bind(origen_txt(p.origen))
        .bind(codigo_txt.clone())
        .bind(codigo_origen)
        .bind(p.peso_empaque.map(Gramos::get))
        .bind(p.vida_util.meses())
        .bind(inst_txt(ahora))
        .execute(&mut *tx)
        .await
        .map_err(mapear_conflicto)?;
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Crear",
            "producto",
            p.id,
            None,
            Some(p.nombre.clone()),
        )
        .await?;
        tx.commit().await?;
        Ok(p)
    }

    async fn producto_por_codigo(
        &self,
        _ctx: &ContextoAcceso,
        codigo: &str,
    ) -> Resultado<Option<Producto>> {
        let row = sqlx::query("SELECT * FROM producto WHERE codigo = ?")
            .bind(codigo)
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(map_producto).transpose()
    }

    async fn desactivar_producto(&self, ctx: &ContextoAcceso, id: Uuid) -> Resultado<()> {
        ctx.requiere(Permiso::GestionarProductos)?;
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;
        let afectadas = sqlx::query(
            "UPDATE producto SET activo = 0, updated_at = ? WHERE id = ? AND activo = 1",
        )
        .bind(inst_txt(ahora))
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if afectadas == 0 {
            return Err(ErrorAlmacen::NoEncontrado(format!("producto activo {id}")));
        }
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Eliminar",
            "producto",
            id,
            Some("activo".into()),
            Some("inactivo".into()),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn fijar_vida_util(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        vida: VidaUtil,
    ) -> Resultado<()> {
        ctx.requiere(Permiso::GestionarProductos)?;
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;
        let antes: i64 = sqlx::query("SELECT vida_util FROM producto WHERE id = ?")
            .bind(producto.to_string())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("producto {producto}")))?
            .try_get("vida_util")?;
        sqlx::query("UPDATE producto SET vida_util = ?, updated_at = ? WHERE id = ?")
            .bind(vida.meses())
            .bind(inst_txt(ahora))
            .bind(producto.to_string())
            .execute(&mut *tx)
            .await?;
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Modificar",
            "producto",
            producto,
            Some(format!("vida_util {antes}")),
            Some(format!("vida_util {}", vida.meses())),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

// ================== impl Precios ==================

impl Precios for Almacen {
    async fn fijar_precio(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
        nivel: Nivel,
        monto: Centavos,
    ) -> Resultado<()> {
        ctx.requiere_en(Permiso::EditarPrecio, sucursal)?;
        if monto.get() < 0 {
            return Err(ErrorDominio::Invalido("el precio no puede ser negativo".into()).into());
        }
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;
        let anterior: Option<i64> = sqlx::query(
            "SELECT monto FROM precio WHERE producto_id = ? AND sucursal_id = ? AND nivel = ?",
        )
        .bind(producto.to_string())
        .bind(sucursal.to_string())
        .bind(nivel_txt(nivel))
        .fetch_optional(&mut *tx)
        .await?
        .map(|r| r.try_get::<i64, _>("monto"))
        .transpose()?;
        sqlx::query(
            "INSERT INTO precio (producto_id, sucursal_id, nivel, monto, updated_at) VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(producto_id, sucursal_id, nivel) DO UPDATE SET monto = excluded.monto, updated_at = excluded.updated_at",
        )
        .bind(producto.to_string())
        .bind(sucursal.to_string())
        .bind(nivel_txt(nivel))
        .bind(monto.get())
        .bind(inst_txt(ahora))
        .execute(&mut *tx)
        .await?;
        let op = if anterior.is_some() {
            "Modificar"
        } else {
            "Crear"
        };
        auditar(
            &mut tx,
            ahora,
            ctx,
            op,
            "precio",
            producto,
            anterior.map(|m| Centavos::new(m).to_string()),
            Some(monto.to_string()),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn copiar_precios(
        &self,
        ctx: &ContextoAcceso,
        origen: Uuid,
        destino: Uuid,
        productos: &[Uuid],
    ) -> Resultado<u64> {
        ctx.requiere_en(Permiso::EditarPrecio, destino)?;
        exigir_alcance(ctx, origen)?;
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;
        let mut copiadas = 0u64;
        for prod in productos {
            let filas = sqlx::query(
                "SELECT nivel, monto FROM precio WHERE producto_id = ? AND sucursal_id = ?",
            )
            .bind(prod.to_string())
            .bind(origen.to_string())
            .fetch_all(&mut *tx)
            .await?;
            for f in &filas {
                let nivel: String = f.try_get("nivel")?;
                let monto: i64 = f.try_get("monto")?;
                sqlx::query(
                    "INSERT INTO precio (producto_id, sucursal_id, nivel, monto, updated_at) VALUES (?, ?, ?, ?, ?) \
                     ON CONFLICT(producto_id, sucursal_id, nivel) DO UPDATE SET monto = excluded.monto, updated_at = excluded.updated_at",
                )
                .bind(prod.to_string())
                .bind(destino.to_string())
                .bind(&nivel)
                .bind(monto)
                .bind(inst_txt(ahora))
                .execute(&mut *tx)
                .await?;
                copiadas += 1;
            }
        }
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Crear",
            "precio",
            destino,
            Some(origen.to_string()),
            Some(format!("{copiadas} precios")),
        )
        .await?;
        tx.commit().await?;
        Ok(copiadas)
    }

    async fn niveles_con_precio(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
    ) -> Resultado<BTreeSet<Nivel>> {
        exigir_alcance(ctx, sucursal)?;
        let filas =
            sqlx::query("SELECT nivel FROM precio WHERE producto_id = ? AND sucursal_id = ?")
                .bind(producto.to_string())
                .bind(sucursal.to_string())
                .fetch_all(&self.pool)
                .await?;
        let mut set = BTreeSet::new();
        for f in &filas {
            set.insert(nivel_de(f.try_get("nivel")?)?);
        }
        Ok(set)
    }

    async fn esta_congelado(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
    ) -> Resultado<bool> {
        Ok(precio::esta_congelado(
            &self.niveles_con_precio(ctx, producto, sucursal).await?,
        ))
    }

    async fn vendible(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
        nivel: Nivel,
    ) -> Resultado<bool> {
        // Un producto inactivo no se vende aunque conserve precios (spec 4.4):
        // la regla vive aquí para que ningún consumidor futuro la olvide (DRY).
        let activo: i64 = sqlx::query("SELECT activo FROM producto WHERE id = ?")
            .bind(producto.to_string())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("producto {producto}")))?
            .try_get("activo")?;
        if activo == 0 {
            return Ok(false);
        }
        Ok(precio::vendible(
            &self.niveles_con_precio(ctx, producto, sucursal).await?,
            nivel,
        ))
    }
}

// ================== impl Caja ==================

impl Caja for Almacen {
    async fn abrir_sesion(
        &self,
        ctx: &ContextoAcceso,
        sucursal: Uuid,
        fondo: Centavos,
    ) -> Resultado<SesionCaja> {
        let ahora = Instante::ahora();
        let s = SesionCaja::abrir(ctx, sucursal, fondo, ahora)?;
        let mut tx = self.pool.begin().await?;
        let abiertas: i64 = sqlx::query(
            "SELECT COUNT(*) AS n FROM sesion_caja WHERE sucursal_id = ? AND estado = 'Abierta'",
        )
        .bind(sucursal.to_string())
        .fetch_one(&mut *tx)
        .await?
        .try_get("n")?;
        if abiertas > 0 {
            return Err(ErrorAlmacen::Conflicto(
                "ya hay una sesión abierta en la sucursal; ciérrala primero".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO sesion_caja (id, sucursal_id, cajero_id, fondo, abierta_en, estado, updated_at) \
             VALUES (?, ?, ?, ?, ?, 'Abierta', ?)",
        )
        .bind(s.id.to_string())
        .bind(s.sucursal.to_string())
        .bind(s.cajero.to_string())
        .bind(s.fondo_apertura.get())
        .bind(inst_txt(s.abierta_en))
        .bind(inst_txt(ahora))
        .execute(&mut *tx)
        .await
        .map_err(mapear_conflicto)?;
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Crear",
            "sesion_caja",
            s.id,
            None,
            Some(s.fondo_apertura.to_string()),
        )
        .await?;
        tx.commit().await?;
        Ok(s)
    }

    async fn registrar_gasto(
        &self,
        ctx: &ContextoAcceso,
        sucursal: Uuid,
        sesion: Uuid,
        monto: Centavos,
        concepto: &str,
    ) -> Resultado<Gasto> {
        let ahora = Instante::ahora();
        let codigo = self.codigo_de_sucursal(sucursal).await?;
        let mut tx = self.pool.begin().await?;
        // El gasto se registra contra la sesión **abierta de su sucursal** (D13).
        // La conciliación posterior (D19) aplica a la *resolución* tardía de la
        // aprobación, nunca al registro: no se registran gastos en un corte cerrado.
        let ses = sqlx::query("SELECT sucursal_id, estado FROM sesion_caja WHERE id = ?")
            .bind(sesion.to_string())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("sesión {sesion}")))?;
        if uuid_de(ses.try_get("sucursal_id")?)? != sucursal {
            return Err(ErrorDominio::Regla(
                "la sesión no pertenece a la sucursal del gasto".into(),
            )
            .into());
        }
        if ses.try_get::<String, _>("estado")? != "Abierta" {
            return Err(ErrorDominio::Regla(
                "la sesión ya cerró; el gasto se registra contra la sesión abierta".into(),
            )
            .into());
        }
        let folio =
            Almacen::siguiente_folio(&mut tx, sucursal, &codigo, TipoDocumento::Gasto).await?;
        let gasto = Gasto::registrar(ctx, folio, monto, concepto, sucursal, sesion, ahora)?;
        let aprob = Aprobacion::pendiente(gasto.id, sucursal, gasto.cajero, ahora);

        sqlx::query(
            "INSERT INTO gasto (id, folio, monto, concepto, cajero_id, sucursal_id, sesion_id, registrado, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(gasto.id.to_string())
        .bind(gasto.folio.get())
        .bind(gasto.monto.get())
        .bind(&gasto.concepto)
        .bind(gasto.cajero.to_string())
        .bind(gasto.sucursal.to_string())
        .bind(gasto.sesion.to_string())
        .bind(inst_txt(gasto.registrado))
        .bind(inst_txt(ahora))
        .execute(&mut *tx)
        .await
        .map_err(mapear_conflicto)?;
        sqlx::query(
            "INSERT INTO aprobacion (id, gasto_id, sucursal_id, solicitante_id, estado, updated_at) \
             VALUES (?, ?, ?, ?, 'Pendiente', ?)",
        )
        .bind(aprob.id.to_string())
        .bind(aprob.gasto.to_string())
        .bind(aprob.sucursal.to_string())
        .bind(aprob.solicitante.to_string())
        .bind(inst_txt(ahora))
        .execute(&mut *tx)
        .await?;
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Crear",
            "gasto",
            gasto.id,
            None,
            Some(format!("{} {}", gasto.folio.get(), gasto.monto)),
        )
        .await?;
        tx.commit().await?;
        Ok(gasto)
    }

    async fn aprobar_gasto(&self, ctx: &ContextoAcceso, gasto: Uuid) -> Resultado<()> {
        self.resolver(ctx, gasto, |ap, ctx, ahora| ap.aprobar(ctx, ahora))
            .await
    }

    async fn rechazar_gasto(
        &self,
        ctx: &ContextoAcceso,
        gasto: Uuid,
        motivo: &str,
    ) -> Resultado<()> {
        self.resolver(ctx, gasto, |ap, ctx, ahora| ap.rechazar(ctx, motivo, ahora))
            .await
    }

    async fn cancelar_gasto(
        &self,
        ctx: &ContextoAcceso,
        gasto: Uuid,
        motivo: &str,
    ) -> Resultado<()> {
        self.resolver(ctx, gasto, |ap, ctx, ahora| ap.cancelar(ctx, motivo, ahora))
            .await
    }

    async fn aprobacion_de(&self, ctx: &ContextoAcceso, gasto: Uuid) -> Resultado<Aprobacion> {
        let row = sqlx::query("SELECT * FROM aprobacion WHERE gasto_id = ?")
            .bind(gasto.to_string())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("aprobación del gasto {gasto}")))?;
        let ap = map_aprobacion(&row)?;
        exigir_alcance(ctx, ap.sucursal)?;
        Ok(ap)
    }

    /// Las aprobaciones `pendientes` **dirigidas a este aprobador**: las de las
    /// sucursales que su alcance cubre. Exige portar `autorizar_gasto` (quien no
    /// puede resolver, no necesita ver la cola).
    async fn aprobaciones_pendientes(&self, ctx: &ContextoAcceso) -> Resultado<Vec<Aprobacion>> {
        if !ctx.tiene(Permiso::AutorizarGasto) {
            return Err(ErrorDominio::PermisoDenegado(Permiso::AutorizarGasto).into());
        }
        let filas =
            sqlx::query("SELECT * FROM aprobacion WHERE estado = 'Pendiente' ORDER BY updated_at")
                .fetch_all(&self.pool)
                .await?;
        let mut out = Vec::new();
        for r in &filas {
            let ap = map_aprobacion(r)?;
            if ctx.alcance.cubre(ap.sucursal) {
                out.push(ap);
            }
        }
        Ok(out)
    }

    async fn listar_gastos(&self, ctx: &ContextoAcceso, sucursal: Uuid) -> Resultado<Vec<Gasto>> {
        exigir_alcance(ctx, sucursal)?;
        let filas = sqlx::query("SELECT * FROM gasto WHERE sucursal_id = ? ORDER BY registrado")
            .bind(sucursal.to_string())
            .fetch_all(&self.pool)
            .await?;
        filas.iter().map(map_gasto).collect()
    }

    async fn cerrar_sesion(
        &self,
        ctx: &ContextoAcceso,
        sesion: Uuid,
        contado: Centavos,
        ventas: ResumenVentas,
    ) -> Resultado<()> {
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT * FROM sesion_caja WHERE id = ?")
            .bind(sesion.to_string())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("sesión {sesion}")))?;
        let sucursal = uuid_de(row.try_get("sucursal_id")?)?;
        let mut s = SesionCaja {
            id: sesion,
            sucursal,
            cajero: uuid_de(row.try_get("cajero_id")?)?,
            fondo_apertura: Centavos::new(row.try_get("fondo")?),
            abierta_en: inst_de(row.try_get("abierta_en")?)?,
            estado: EstadoSesion::Abierta,
            corte: None,
            cerrada_en: None,
            actualizado: ahora,
        };
        if row.try_get::<String, _>("estado")? == "Cerrada" {
            return Err(
                ErrorDominio::TransicionInvalida("la sesión ya está cerrada".into()).into(),
            );
        }
        // Solo los gastos autorizados de esta sesión restan del esperado (D13).
        let autorizados: i64 = sqlx::query(
            "SELECT COALESCE(SUM(g.monto), 0) AS total FROM gasto g \
             JOIN aprobacion a ON a.gasto_id = g.id WHERE g.sesion_id = ? AND a.estado = 'Aprobada'",
        )
        .bind(sesion.to_string())
        .fetch_one(&mut *tx)
        .await?
        .try_get("total")?;
        // El código se lee con la conexión de la transacción (el pool tiene una
        // sola conexión: pedir otra aquí se auto-bloquearía).
        let codigo_txt: String = sqlx::query("SELECT codigo FROM sucursal WHERE id = ?")
            .bind(sucursal.to_string())
            .fetch_one(&mut *tx)
            .await?
            .try_get("codigo")?;
        let codigo = CodigoSucursal::nueva(&codigo_txt)?;
        let folio =
            Almacen::siguiente_folio(&mut tx, sucursal, &codigo, TipoDocumento::Corte).await?;
        s.cerrar(
            ctx,
            contado,
            ventas,
            Centavos::new(autorizados),
            folio,
            ahora,
        )?;
        let corte = s.corte.as_ref().expect("cerrar deja corte");

        sqlx::query(
            "UPDATE sesion_caja SET estado = 'Cerrada', contado = ?, esperado = ?, diferencia = ?, \
             gastos_autorizados = ?, ventas_efectivo = ?, ventas_transferencia = ?, ventas_deposito = ?, \
             folio_corte = ?, cerrada_en = ?, updated_at = ? WHERE id = ?",
        )
        .bind(corte.contado.get())
        .bind(corte.esperado.get())
        .bind(corte.diferencia.get())
        .bind(corte.gastos_autorizados.get())
        .bind(corte.ventas.efectivo.get())
        .bind(corte.ventas.transferencia.get())
        .bind(corte.ventas.deposito.get())
        .bind(corte.folio.get())
        .bind(inst_txt(ahora))
        .bind(inst_txt(ahora))
        .bind(sesion.to_string())
        .execute(&mut *tx)
        .await?;
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Modificar",
            "sesion_caja",
            sesion,
            Some("Abierta".into()),
            Some(corte.folio.get().to_string()),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn conciliacion(&self, ctx: &ContextoAcceso, sesion: Uuid) -> Resultado<Corte> {
        let row = sqlx::query("SELECT * FROM sesion_caja WHERE id = ?")
            .bind(sesion.to_string())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("sesión {sesion}")))?;
        let sucursal = uuid_de(row.try_get("sucursal_id")?)?;
        ctx.requiere_en(Permiso::VerConciliacion, sucursal)?;
        if row.try_get::<String, _>("estado")? != "Cerrada" {
            return Err(ErrorDominio::Regla("la sesión aún no tiene corte".into()).into());
        }
        Ok(Corte {
            folio: Folio::desde_texto(row.try_get("folio_corte")?),
            fondo_apertura: Centavos::new(row.try_get("fondo")?),
            ventas: ResumenVentas {
                efectivo: Centavos::new(row.try_get("ventas_efectivo")?),
                transferencia: Centavos::new(row.try_get("ventas_transferencia")?),
                deposito: Centavos::new(row.try_get("ventas_deposito")?),
            },
            gastos_autorizados: Centavos::new(row.try_get("gastos_autorizados")?),
            esperado: Centavos::new(row.try_get("esperado")?),
            contado: Centavos::new(row.try_get("contado")?),
            diferencia: Centavos::new(row.try_get("diferencia")?),
        })
    }
}

impl Almacen {
    /// Carga la aprobación de un gasto, le aplica la transición del dominio y la
    /// persiste con auditoría. Centraliza aprobar/rechazar/cancelar (DRY).
    async fn resolver(
        &self,
        ctx: &ContextoAcceso,
        gasto: Uuid,
        transicion: impl FnOnce(&mut Aprobacion, &ContextoAcceso, Instante) -> Result<(), ErrorDominio>,
    ) -> Resultado<()> {
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT * FROM aprobacion WHERE gasto_id = ?")
            .bind(gasto.to_string())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("aprobación del gasto {gasto}")))?;
        let mut ap = map_aprobacion(&row)?;
        let antes = estado_aprob_txt(ap.estado).to_string();
        transicion(&mut ap, ctx, ahora)?;
        sqlx::query("UPDATE aprobacion SET estado = ?, resolutor_id = ?, resuelto_en = ?, motivo = ?, updated_at = ? WHERE id = ?")
            .bind(estado_aprob_txt(ap.estado))
            .bind(ap.resolutor.map(|u| u.to_string()))
            .bind(ap.resuelto_en.map(inst_txt))
            .bind(ap.motivo.clone())
            .bind(inst_txt(ahora))
            .bind(ap.id.to_string())
            .execute(&mut *tx)
            .await?;
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Modificar",
            "aprobacion",
            ap.id,
            Some(antes),
            Some(estado_aprob_txt(ap.estado).to_string()),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

// ================== impl Inventario ==================

impl Inventario for Almacen {
    async fn ajustar(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
        objetivo: Cantidad,
        motivo: &str,
    ) -> Resultado<()> {
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;
        let p = producto_tx(&mut tx, producto).await?;
        let saldo = saldo_tx(&mut tx, producto, sucursal, p.unidad_venta()).await?;
        // El dominio valida permiso+alcance, unidad, objetivo ≥ 0 y motivo (D25/D26).
        let mov = MovimientoInventario::ajuste(ctx, &p, sucursal, objetivo, saldo, motivo, ahora)?;
        // Ajuste absoluto: el nuevo saldo ES el objetivo.
        fijar_existencia(&mut tx, producto, sucursal, objetivo.magnitud(), ahora).await?;
        insertar_movimiento(&mut tx, &mov).await?;
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Modificar",
            "inventario",
            producto,
            Some(saldo.magnitud().to_string()),
            Some(format!(
                "{} (suc {sucursal}; {})",
                objetivo.magnitud(),
                mov.motivo.as_deref().unwrap_or("")
            )),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn revertir_movimiento(&self, ctx: &ContextoAcceso, movimiento: Uuid) -> Resultado<()> {
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT * FROM movimiento_inventario WHERE id = ?")
            .bind(movimiento.to_string())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("movimiento {movimiento}")))?;
        let sucursal = uuid_de(row.try_get("sucursal_id")?)?;
        let producto = uuid_de(row.try_get("producto_id")?)?;
        ctx.requiere_en(Permiso::AjustarInventario, sucursal)?;
        let p = producto_tx(&mut tx, producto).await?;
        let mut mov = map_movimiento(&row, p.unidad_venta())?;
        if mov.tipo != TipoMovimiento::Ajuste {
            return Err(ErrorDominio::Regla(
                "en esta capa solo se revierten movimientos de ajuste".into(),
            )
            .into());
        }
        let saldo = saldo_tx(&mut tx, producto, sucursal, p.unidad_venta()).await?;
        // La compensación no puede dejar la existencia negativa (D25).
        let nueva = Existencia::nueva(producto, sucursal, saldo).aplicar(mov.compensacion())?;
        mov.alternar_reversion(ahora);
        sqlx::query("UPDATE movimiento_inventario SET estado = ?, updated_at = ? WHERE id = ?")
            .bind(estado_mov_txt(mov.estado))
            .bind(inst_txt(ahora))
            .bind(movimiento.to_string())
            .execute(&mut *tx)
            .await?;
        fijar_existencia(
            &mut tx,
            producto,
            sucursal,
            nueva.cantidad.magnitud(),
            ahora,
        )
        .await?;
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Modificar",
            "inventario",
            producto,
            Some(saldo.magnitud().to_string()),
            Some(format!(
                "{} ({} mov {movimiento})",
                nueva.cantidad.magnitud(),
                estado_mov_txt(mov.estado)
            )),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn existencia(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
    ) -> Resultado<Cantidad> {
        ctx.requiere_en(Permiso::VerInventario, sucursal)?;
        let unidad = self.unidad_de_producto(producto).await?;
        let row = sqlx::query(
            "SELECT cantidad FROM existencia WHERE producto_id = ? AND sucursal_id = ?",
        )
        .bind(producto.to_string())
        .bind(sucursal.to_string())
        .fetch_optional(&self.pool)
        .await?;
        Ok(match row {
            Some(r) => Cantidad::en(unidad, r.try_get("cantidad")?),
            None => Cantidad::cero(unidad),
        })
    }

    async fn movimientos_de(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
    ) -> Resultado<Vec<MovimientoInventario>> {
        ctx.requiere_en(Permiso::VerInventario, sucursal)?;
        let unidad = self.unidad_de_producto(producto).await?;
        let filas = sqlx::query(
            "SELECT * FROM movimiento_inventario WHERE producto_id = ? AND sucursal_id = ? ORDER BY registrado",
        )
        .bind(producto.to_string())
        .bind(sucursal.to_string())
        .fetch_all(&self.pool)
        .await?;
        filas.iter().map(|r| map_movimiento(r, unidad)).collect()
    }
}

impl Almacen {
    /// Unidad de venta de un producto (para interpretar la magnitud de la existencia).
    async fn unidad_de_producto(&self, producto: Uuid) -> Resultado<UnidadVenta> {
        let tipo: String = sqlx::query("SELECT tipo FROM producto WHERE id = ?")
            .bind(producto.to_string())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("producto {producto}")))?
            .try_get("tipo")?;
        Ok(tipo_prod_de(&tipo)?.unidad_venta())
    }
}

// ================== impl Etiquetado ==================

impl Almacen {
    /// Siguiente discriminador per-ítem: secuencia persistida módulo 10⁵; si el
    /// candidato colisiona con una etiqueta **activa**, avanza y reintenta (D31).
    async fn discriminador_libre(conn: &mut sqlx::SqliteConnection) -> Resultado<u64> {
        // Tope de reintentos: con 10⁵ valores, agotar el espacio significa esa
        // cantidad de etiquetas activas del mismo módulo — se reporta, no se cuelga.
        const INTENTOS: u64 = 10_000;
        for _ in 0..INTENTOS {
            let row = sqlx::query(
                "INSERT INTO discriminador_seq (id, siguiente) VALUES (1, 1) \
                 ON CONFLICT(id) DO UPDATE SET siguiente = siguiente + 1 RETURNING siguiente",
            )
            .fetch_one(&mut *conn)
            .await?;
            let crudo: i64 = row.try_get("siguiente")?;
            let candidato = (crudo as u64) % MODULO_DISCRIMINADOR;
            let ocupado = sqlx::query(
                "SELECT 1 AS x FROM etiqueta WHERE discriminador = ? AND estado = 'Activa' LIMIT 1",
            )
            .bind(candidato as i64)
            .fetch_optional(&mut *conn)
            .await?;
            if ocupado.is_none() {
                return Ok(candidato);
            }
        }
        Err(ErrorAlmacen::Conflicto(
            "sin discriminadores libres para etiquetas".into(),
        ))
    }
}

async fn insertar_etiqueta(conn: &mut sqlx::SqliteConnection, e: &Etiqueta) -> Resultado<()> {
    sqlx::query(
        "INSERT INTO etiqueta (id, codigo, discriminador, producto_id, sucursal_id, caja_id, peso, fecha_etiquetado, caducidad, estado, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(e.id.to_string())
    .bind(e.codigo.get())
    .bind(e.discriminador as i64)
    .bind(e.producto.to_string())
    .bind(e.sucursal.to_string())
    .bind(e.caja.map(|c| c.to_string()))
    .bind(e.peso.get())
    .bind(inst_txt(e.fecha_etiquetado))
    .bind(e.caducidad.to_string())
    .bind(estado_eti_txt(e.estado))
    .bind(inst_txt(e.actualizado))
    .execute(conn)
    .await
    .map_err(mapear_conflicto)?;
    Ok(())
}

impl Etiquetado for Almacen {
    async fn etiquetar_pesadas(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
        pesos: &[Gramos],
    ) -> Resultado<Vec<Etiqueta>> {
        if pesos.is_empty() {
            return Err(ErrorDominio::Invalido("no hay pesadas que etiquetar".into()).into());
        }
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;
        let p = producto_tx(&mut tx, producto).await?;
        let zona = zona_tx(&mut tx, sucursal).await?;
        let mut etiquetas = Vec::with_capacity(pesos.len());
        for &peso in pesos {
            let discriminador = Almacen::discriminador_libre(&mut tx).await?;
            // El dominio valida permiso+alcance, tipo peso-variable y rango del
            // peso, y congela la caducidad (D31/D32).
            let e = Etiqueta::nueva(ctx, &p, sucursal, &zona, peso, discriminador, ahora)?;
            insertar_etiqueta(&mut tx, &e).await?;
            auditar(
                &mut tx,
                ahora,
                ctx,
                "Crear",
                "etiqueta",
                e.id,
                None,
                Some(format!("{} {}", e.codigo.get(), e.peso)),
            )
            .await?;
            etiquetas.push(e);
        }
        tx.commit().await?;
        Ok(etiquetas)
    }

    async fn cerrar_caja_pesadas(
        &self,
        ctx: &ContextoAcceso,
        etiquetas: &[Uuid],
    ) -> Resultado<CajaEtiquetado> {
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;
        let mut cargadas = Vec::with_capacity(etiquetas.len());
        for id in etiquetas {
            let row = sqlx::query("SELECT * FROM etiqueta WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("etiqueta {id}")))?;
            cargadas.push(map_etiqueta(&row)?);
        }
        let producto = cargadas
            .first()
            .map(|e| e.producto)
            .ok_or_else(|| ErrorDominio::Invalido("la caja no tiene etiquetas".into()))?;
        let p = producto_tx(&mut tx, producto).await?;
        // El dominio valida permiso+alcance, homogeneidad, estado y caja previa (D35).
        let caja = CajaEtiquetado::cerrar_pesadas(ctx, &p, &cargadas, ahora)?;
        sqlx::query(
            "INSERT INTO caja_etiquetado (id, producto_id, sucursal_id, cantidad, fecha_etiquetado, caducidad, estado, updated_at) \
             VALUES (?, ?, ?, NULL, ?, ?, ?, ?)",
        )
        .bind(caja.id.to_string())
        .bind(caja.producto.to_string())
        .bind(caja.sucursal.to_string())
        .bind(inst_txt(caja.fecha_etiquetado))
        .bind(caja.caducidad.map(|c| c.to_string()))
        .bind(estado_eti_txt(caja.estado))
        .bind(inst_txt(caja.actualizado))
        .execute(&mut *tx)
        .await?;
        for e in &cargadas {
            sqlx::query("UPDATE etiqueta SET caja_id = ?, updated_at = ? WHERE id = ?")
                .bind(caja.id.to_string())
                .bind(inst_txt(ahora))
                .bind(e.id.to_string())
                .execute(&mut *tx)
                .await?;
        }
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Crear",
            "caja_etiquetado",
            caja.id,
            None,
            Some(format!("{} etiquetas", cargadas.len())),
        )
        .await?;
        tx.commit().await?;
        Ok(caja)
    }

    async fn cerrar_caja_pieza(
        &self,
        ctx: &ContextoAcceso,
        producto: Uuid,
        sucursal: Uuid,
        cantidad: i64,
    ) -> Resultado<CajaEtiquetado> {
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;
        let p = producto_tx(&mut tx, producto).await?;
        let zona = zona_tx(&mut tx, sucursal).await?;
        // El dominio valida permiso+alcance, tipo pieza, cantidad positiva y la
        // caducidad según el origen (D35).
        let caja = CajaEtiquetado::cerrar_pieza(ctx, &p, sucursal, &zona, cantidad, ahora)?;
        sqlx::query(
            "INSERT INTO caja_etiquetado (id, producto_id, sucursal_id, cantidad, fecha_etiquetado, caducidad, estado, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(caja.id.to_string())
        .bind(caja.producto.to_string())
        .bind(caja.sucursal.to_string())
        .bind(caja.cantidad)
        .bind(inst_txt(caja.fecha_etiquetado))
        .bind(caja.caducidad.map(|c| c.to_string()))
        .bind(estado_eti_txt(caja.estado))
        .bind(inst_txt(caja.actualizado))
        .execute(&mut *tx)
        .await?;
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Crear",
            "caja_etiquetado",
            caja.id,
            None,
            Some(format!("{cantidad} pzas")),
        )
        .await?;
        tx.commit().await?;
        Ok(caja)
    }

    async fn etiqueta_por_codigo(
        &self,
        ctx: &ContextoAcceso,
        codigo: &str,
    ) -> Resultado<Option<Etiqueta>> {
        let row = sqlx::query("SELECT * FROM etiqueta WHERE codigo = ?")
            .bind(codigo)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            None => Ok(None),
            Some(r) => {
                let e = map_etiqueta(&r)?;
                Ok(if ctx.alcance.cubre(e.sucursal) {
                    Some(e)
                } else {
                    None
                })
            }
        }
    }

    async fn etiquetas_de_caja(
        &self,
        ctx: &ContextoAcceso,
        caja: Uuid,
    ) -> Resultado<Vec<Etiqueta>> {
        let row = sqlx::query("SELECT sucursal_id FROM caja_etiquetado WHERE id = ?")
            .bind(caja.to_string())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("caja {caja}")))?;
        exigir_alcance(ctx, uuid_de(row.try_get("sucursal_id")?)?)?;
        let filas = sqlx::query("SELECT * FROM etiqueta WHERE caja_id = ? ORDER BY updated_at")
            .bind(caja.to_string())
            .fetch_all(&self.pool)
            .await?;
        filas.iter().map(map_etiqueta).collect()
    }

    async fn barrer_por_vencer(
        &self,
        ctx: &ContextoAcceso,
        referencia: Instante,
    ) -> Resultado<u64> {
        if ctx.actor != Actor::Sistema {
            return Err(ErrorDominio::Regla(
                "el barrido por vencer es una operación del sistema".into(),
            )
            .into());
        }
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;
        let matriz: Uuid = {
            let row = sqlx::query("SELECT id FROM sucursal WHERE tipo = 'Matriz'")
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| ErrorAlmacen::NoEncontrado("sucursal matriz".into()))?;
            uuid_de(row.try_get("id")?)?
        };
        let expendios = sqlx::query(
            "SELECT id, nombre, zona FROM sucursal WHERE tipo = 'Expendio' AND activo = 1",
        )
        .fetch_all(&mut *tx)
        .await?;

        let mut emitidas = 0u64;
        for exp in &expendios {
            let sucursal = uuid_de(exp.try_get("id")?)?;
            let nombre_suc: String = exp.try_get("nombre")?;
            let zona = ZonaHoraria::nueva(exp.try_get("zona")?)?;
            // La ventana se corta en el día local de la sucursal (D21/D36);
            // incluye lo ya vencido (sigue "por vencer" hasta que se atienda).
            let limite = referencia
                .dia_local(&zona)
                .checked_add(Span::new().days(DIAS_ALERTA_POR_VENCER))
                .map_err(corrupto)?
                .to_string();

            // Etiquetas por-ítem (peso_variable), agrupadas por producto.
            let grupos = sqlx::query(
                "SELECT e.producto_id, COUNT(*) AS n, SUM(e.peso) AS peso, MIN(e.caducidad) AS cad, p.nombre AS nombre \
                 FROM etiqueta e JOIN producto p ON p.id = e.producto_id \
                 WHERE e.sucursal_id = ? AND e.estado = 'Activa' AND e.caducidad <= ? \
                 GROUP BY e.producto_id",
            )
            .bind(sucursal.to_string())
            .bind(&limite)
            .fetch_all(&mut *tx)
            .await?;
            for g in &grupos {
                let n: i64 = g.try_get("n")?;
                let peso: i64 = g.try_get("peso")?;
                let nombre: String = g.try_get("nombre")?;
                let mensaje = format!(
                    "{n} etiquetas ({}) de {nombre} por vencer en {nombre_suc}",
                    Gramos::new(peso)
                );
                emitidas += Almacen::notificar_grupo_por_vencer(
                    &mut tx,
                    ctx,
                    ahora,
                    uuid_de(g.try_get("producto_id")?)?,
                    sucursal,
                    matriz,
                    g.try_get("cad")?,
                    &mensaje,
                )
                .await?;
            }

            // Cajas de pieza (cantidad), solo las que llevan caducidad propia (D35).
            let grupos = sqlx::query(
                "SELECT c.producto_id, SUM(c.cantidad) AS n, MIN(c.caducidad) AS cad, p.nombre AS nombre \
                 FROM caja_etiquetado c JOIN producto p ON p.id = c.producto_id \
                 WHERE c.sucursal_id = ? AND c.estado = 'Activa' AND c.cantidad IS NOT NULL \
                   AND c.caducidad IS NOT NULL AND c.caducidad <= ? \
                 GROUP BY c.producto_id",
            )
            .bind(sucursal.to_string())
            .bind(&limite)
            .fetch_all(&mut *tx)
            .await?;
            for g in &grupos {
                let n: i64 = g.try_get("n")?;
                let nombre: String = g.try_get("nombre")?;
                let mensaje = format!("{n} pzas {nombre} por vencer en {nombre_suc}");
                emitidas += Almacen::notificar_grupo_por_vencer(
                    &mut tx,
                    ctx,
                    ahora,
                    uuid_de(g.try_get("producto_id")?)?,
                    sucursal,
                    matriz,
                    g.try_get("cad")?,
                    &mensaje,
                )
                .await?;
            }
        }
        tx.commit().await?;
        Ok(emitidas)
    }
}

impl Almacen {
    /// Emite la alerta de un grupo `producto × expendio` a **ambas** bandejas
    /// (sucursal afectada y matriz), una sola vez por lote: la clave de grupo
    /// incluye la caducidad más próxima, así el mismo lote no se re-notifica y
    /// un lote nuevo sí (D36).
    #[allow(clippy::too_many_arguments)] // firma del grupo: producto/sucursal/matriz/lote es intrínseca
    async fn notificar_grupo_por_vencer(
        tx: &mut sqlx::SqliteTransaction<'_>,
        ctx: &ContextoAcceso,
        ahora: Instante,
        producto: Uuid,
        sucursal: Uuid,
        matriz: Uuid,
        caducidad_min: String,
        mensaje: &str,
    ) -> Resultado<u64> {
        let grupo = format!("por_vencer:{producto}:{sucursal}:{caducidad_min}");
        let vigente = sqlx::query("SELECT 1 AS x FROM notificacion WHERE grupo = ? LIMIT 1")
            .bind(&grupo)
            .fetch_optional(&mut **tx)
            .await?;
        if vigente.is_some() {
            return Ok(0);
        }
        let mut emitidas = 0u64;
        for destino in [sucursal, matriz] {
            let n =
                Notificacion::emitir(ctx, TipoNotificacion::PorVencer, mensaje, destino, ahora)?;
            insertar_notificacion(&mut *tx, &n, Some(&grupo)).await?;
            auditar(
                tx,
                ahora,
                ctx,
                "Crear",
                "notificacion",
                n.id,
                None,
                Some(mensaje.to_string()),
            )
            .await?;
            emitidas += 1;
        }
        Ok(emitidas)
    }
}

// ================== impl Notificaciones ==================

impl Notificaciones for Almacen {
    async fn emitir(
        &self,
        ctx: &ContextoAcceso,
        tipo: TipoNotificacion,
        mensaje: &str,
        sucursal: Uuid,
    ) -> Resultado<Notificacion> {
        let ahora = Instante::ahora();
        // El dominio exige actor `sistema` y mensaje no vacío (D37).
        let n = Notificacion::emitir(ctx, tipo, mensaje, sucursal, ahora)?;
        let mut tx = self.pool.begin().await?;
        insertar_notificacion(&mut tx, &n, None).await?;
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Crear",
            "notificacion",
            n.id,
            None,
            Some(n.mensaje.clone()),
        )
        .await?;
        tx.commit().await?;
        Ok(n)
    }

    async fn bandeja(&self, ctx: &ContextoAcceso, sucursal: Uuid) -> Resultado<Vec<Notificacion>> {
        ctx.requiere_en(Permiso::VerNotificaciones, sucursal)?;
        let filas = sqlx::query("SELECT * FROM notificacion WHERE sucursal_id = ? ORDER BY creado")
            .bind(sucursal.to_string())
            .fetch_all(&self.pool)
            .await?;
        filas.iter().map(map_notificacion).collect()
    }

    async fn marcar_leida(&self, ctx: &ContextoAcceso, notificacion: Uuid) -> Resultado<()> {
        let ahora = Instante::ahora();
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT * FROM notificacion WHERE id = ?")
            .bind(notificacion.to_string())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ErrorAlmacen::NoEncontrado(format!("notificación {notificacion}")))?;
        let mut n = map_notificacion(&row)?;
        ctx.requiere_en(Permiso::VerNotificaciones, n.sucursal)?;
        if n.leida() {
            // Idempotente: conserva la marca original y no falla (D37).
            return Ok(());
        }
        n.marcar_leida(ahora);
        sqlx::query("UPDATE notificacion SET leida_en = ?, updated_at = ? WHERE id = ?")
            .bind(n.leida_en.map(inst_txt))
            .bind(inst_txt(ahora))
            .bind(notificacion.to_string())
            .execute(&mut *tx)
            .await?;
        auditar(
            &mut tx,
            ahora,
            ctx,
            "Modificar",
            "notificacion",
            notificacion,
            Some("no leída".into()),
            Some("leída".into()),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

// ================== impl Auditoria ==================

impl Auditoria for Almacen {
    /// DECISIÓN ABIERTA (design.md · Open Questions): el *gating* de lectura de
    /// la bitácora —¿permiso `ver_bitacora` por-sucursal o de-sistema?— se
    /// decide al construir la vista de auditoría de la cara admin. Hoy la
    /// lectura no exige permiso; la **escritura** sí es transversal e inmutable
    /// (D11), que es la garantía que la fundación debe dar.
    async fn bitacora_de(
        &self,
        _ctx: &ContextoAcceso,
        entidad: &str,
        id: Uuid,
    ) -> Resultado<Vec<EntradaBitacora>> {
        let filas = sqlx::query(
            "SELECT actor, operacion, entidad, entidad_id, antes, despues, ts FROM bitacora \
             WHERE entidad = ? AND entidad_id = ? ORDER BY ts",
        )
        .bind(entidad)
        .bind(id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::new();
        for r in &filas {
            out.push(EntradaBitacora {
                actor: r.try_get("actor")?,
                operacion: r.try_get("operacion")?,
                entidad: r.try_get("entidad")?,
                entidad_id: uuid_de(r.try_get("entidad_id")?)?,
                antes: r.try_get("antes")?,
                despues: r.try_get("despues")?,
                ts: r.try_get("ts")?,
            });
        }
        Ok(out)
    }
}

/// Registra una operación mutante en la bitácora append-only (D11). Se invoca en
/// la misma transacción que la mutación, de modo que ninguna escritura queda sin
/// atribuir a su actor (usuario o `sistema`).
#[allow(clippy::too_many_arguments)] // firma de bitácora: actor/op/entidad/antes/después es intrínseca
async fn auditar(
    tx: &mut sqlx::SqliteTransaction<'_>,
    ahora: Instante,
    ctx: &ContextoAcceso,
    operacion: &str,
    entidad: &str,
    entidad_id: Uuid,
    antes: Option<String>,
    despues: Option<String>,
) -> Resultado<()> {
    sqlx::query(
        "INSERT INTO bitacora (id, ts, actor, operacion, entidad, entidad_id, alcance, antes, despues) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(inst_txt(ahora))
    .bind(ctx.actor.to_string())
    .bind(operacion)
    .bind(entidad)
    .bind(entidad_id.to_string())
    .bind(alcance_json(&ctx.alcance))
    .bind(antes)
    .bind(despues)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
