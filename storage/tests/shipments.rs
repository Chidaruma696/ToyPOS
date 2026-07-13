//! Pruebas de integración de `branch-shipments` (grupo 3 de `tasks.md`):
//! surtido, devolución, discrepancia, contenido comprometido, permisos/alcance
//! y la alerta de caducidad encendida por la recepción.

use domain::acceso::{Alcance, ContextoAcceso, Permiso};
use domain::envio::EstadoEnvio;
use domain::etiqueta::EstadoEtiqueta;
use domain::inventario::{Cantidad, TipoMovimiento};
use domain::producto::{OrigenProducto, TipoProducto};
use domain::sucursal::{CodigoSucursal, TipoSucursal};
use domain::tiempo::{Instante, ZonaHoraria};
use domain::unidades::Gramos;
use jiff::Span;
use jiff::civil::Date;
use storage::prelude::*;
use uuid::Uuid;

fn url_temporal() -> String {
    let p = std::env::temp_dir().join(format!("toypos-env-{}.db", Uuid::new_v4()));
    format!("sqlite://{}", p.display())
}

async fn con_admin() -> (Almacen, ContextoAcceso, Uuid) {
    let alm = Almacen::abrir(&url_temporal()).await.unwrap();
    let (matriz, _) = alm.bootstrap("MAT", "admin", "clave123").await.unwrap();
    let admin = alm.autenticar("admin", "clave123").await.unwrap();
    (alm, admin, matriz.id)
}

async fn expendio(alm: &Almacen, admin: &ContextoAcceso, codigo: &str) -> Uuid {
    alm.crear_sucursal(
        admin,
        TipoSucursal::Expendio,
        CodigoSucursal::nueva(codigo).unwrap(),
        codigo,
        ZonaHoraria::default(),
    )
    .await
    .unwrap()
    .id
}

async fn producto_peso(alm: &Almacen, admin: &ContextoAcceso, nombre: &str) -> Uuid {
    alm.crear_producto(
        admin,
        BorradorProducto {
            nombre: nombre.into(),
            tipo: TipoProducto::PesoVariable,
            origen: OrigenProducto::Matriz,
            codigo: AltaCodigo::Ninguno,
            peso_empaque: None,
            vida_util: None,
        },
    )
    .await
    .unwrap()
    .id
}

async fn producto_pieza(alm: &Almacen, admin: &ContextoAcceso, nombre: &str) -> Uuid {
    alm.crear_producto(
        admin,
        BorradorProducto {
            nombre: nombre.into(),
            tipo: TipoProducto::Pieza,
            origen: OrigenProducto::Matriz,
            codigo: AltaCodigo::GenerarMatriz,
            peso_empaque: None,
            vida_util: None,
        },
    )
    .await
    .unwrap()
    .id
}

/// Usuario con permisos acotados a una sucursal.
async fn usuario(
    alm: &Almacen,
    admin: &ContextoAcceso,
    nombre: &str,
    permisos: &[Permiso],
    sucursal: Uuid,
) -> ContextoAcceso {
    let rol = alm
        .crear_rol(admin, &format!("rol-{nombre}"), permisos)
        .await
        .unwrap();
    alm.crear_usuario(
        admin,
        nombre,
        "clave123",
        &[rol.id],
        Alcance::de([sucursal]),
    )
    .await
    .unwrap();
    alm.autenticar(nombre, "clave123").await.unwrap()
}

fn instante_antes_de(fecha: Date, dias_antes: i64) -> Instante {
    let dia = fecha.checked_sub(Span::new().days(dias_antes)).unwrap();
    let zoned = dia.at(12, 0, 0, 0).in_tz("America/Mexico_City").unwrap();
    Instante::desde_timestamp(zoned.timestamp())
}

// -------- 3.4 Surtido completo: el stock nace al recibir --------

#[tokio::test]
async fn surtido_completo_hace_nacer_el_stock() {
    let (alm, admin, matriz) = con_admin().await;
    let exp = expendio(&alm, &admin, "SNA").await;
    let peso = producto_peso(&alm, &admin, "tripa de pollo").await;
    let pieza = producto_pieza(&alm, &admin, "queso 500 g").await;

    // Matriz etiqueta: una caja de pesadas (2 500 g), dos sueltas (1 500 g) y
    // una caja de pieza (24).
    let en_caja = alm
        .etiquetar_pesadas(
            &admin,
            peso,
            matriz,
            &[Gramos::new(900), Gramos::new(1100), Gramos::new(500)],
        )
        .await
        .unwrap();
    let ids: Vec<Uuid> = en_caja.iter().map(|e| e.id).collect();
    let caja_pesadas = alm.cerrar_caja_pesadas(&admin, &ids).await.unwrap();
    let sueltas = alm
        .etiquetar_pesadas(&admin, peso, matriz, &[Gramos::new(700), Gramos::new(800)])
        .await
        .unwrap();
    let caja_pieza = alm
        .cerrar_caja_pieza(&admin, pieza, matriz, 24)
        .await
        .unwrap();

    let envio = alm
        .preparar_envio(
            &admin,
            matriz,
            exp,
            &[caja_pesadas.id, caja_pieza.id],
            &[sueltas[0].id, sueltas[1].id],
        )
        .await
        .unwrap();
    assert_eq!(envio.folio.get(), "MATE1");
    alm.marcar_enviado(&admin, envio.id).await.unwrap();
    // Enviar un surtido no toca stock en ningún lado (matriz no lleva, D40).
    assert_eq!(
        alm.existencia(&admin, peso, exp).await.unwrap(),
        Cantidad::Gramos(Gramos::CERO)
    );

    // Recepción completa: cajas + sueltas presentes.
    let recibido = alm
        .recibir_envio(
            &admin,
            envio.id,
            &[caja_pesadas.id, caja_pieza.id, sueltas[0].id, sueltas[1].id],
        )
        .await
        .unwrap();
    assert_eq!(recibido.estado, EstadoEnvio::Recibido);
    assert!(recibido.discrepancia.is_none());

    // El stock del expendio nace con lo presente.
    assert_eq!(
        alm.existencia(&admin, peso, exp).await.unwrap(),
        Cantidad::Gramos(Gramos::new(4000)) // 2500 en caja + 1500 sueltas
    );
    assert_eq!(
        alm.existencia(&admin, pieza, exp).await.unwrap(),
        Cantidad::Unidades(24)
    );
    // Movimientos `recepcion` citando el folio; la matriz sin movimientos.
    let movs = alm.movimientos_de(&admin, peso, exp).await.unwrap();
    assert_eq!(movs.len(), 1);
    assert_eq!(movs[0].tipo, TipoMovimiento::Recepcion);
    assert_eq!(movs[0].motivo.as_deref(), Some("MATE1"));
    assert!(
        alm.movimientos_de(&admin, peso, matriz)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        alm.movimientos_de(&admin, pieza, matriz)
            .await
            .unwrap()
            .is_empty()
    );

    // Las etiquetas viajaron conservando identidad y caducidad.
    let viajada = alm
        .etiqueta_por_codigo(&admin, sueltas[0].codigo.get())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(viajada.sucursal, exp);
    assert_eq!(viajada.caducidad, sueltas[0].caducidad);

    // El documento se consulta por folio con su contenido.
    let (doc, contenido) = alm.envio_por_folio(&admin, "MATE1").await.unwrap().unwrap();
    assert_eq!(doc.estado, EstadoEnvio::Recibido);
    assert_eq!(contenido.cajas.len(), 2);
    assert_eq!(contenido.etiquetas_sueltas.len(), 2);
}

// -------- 3.4 Devolución: descuenta al enviar, con no-negatividad --------

#[tokio::test]
async fn devolucion_descuenta_al_enviar_y_respeta_no_negatividad() {
    let (alm, admin, matriz) = con_admin().await;
    let exp = expendio(&alm, &admin, "SNA").await;
    let pieza = producto_pieza(&alm, &admin, "queso 500 g").await;

    // Surtir 24 piezas al expendio.
    let caja = alm
        .cerrar_caja_pieza(&admin, pieza, matriz, 24)
        .await
        .unwrap();
    let envio = alm
        .preparar_envio(&admin, matriz, exp, &[caja.id], &[])
        .await
        .unwrap();
    alm.marcar_enviado(&admin, envio.id).await.unwrap();
    alm.recibir_envio(&admin, envio.id, &[caja.id])
        .await
        .unwrap();
    assert_eq!(
        alm.existencia(&admin, pieza, exp).await.unwrap(),
        Cantidad::Unidades(24)
    );

    // Devolver la caja: el stock sale del expendio AL ENVIAR.
    let devolucion = alm
        .preparar_envio(&admin, exp, matriz, &[caja.id], &[])
        .await
        .unwrap();
    alm.marcar_enviado(&admin, devolucion.id).await.unwrap();
    assert_eq!(
        alm.existencia(&admin, pieza, exp).await.unwrap(),
        Cantidad::Unidades(0)
    );
    let movs = alm.movimientos_de(&admin, pieza, exp).await.unwrap();
    assert!(movs.iter().any(|m| m.tipo == TipoMovimiento::Envio));
    // Recibir en matriz no postea nada: matriz sigue sin inventario (D34/D40).
    alm.recibir_envio(&admin, devolucion.id, &[caja.id])
        .await
        .unwrap();
    assert!(
        alm.movimientos_de(&admin, pieza, matriz)
            .await
            .unwrap()
            .is_empty()
    );

    // No-negatividad: surtir 16, mermar a 5 por conteo, intentar devolver 16.
    let caja16 = alm
        .cerrar_caja_pieza(&admin, pieza, matriz, 16)
        .await
        .unwrap();
    let envio2 = alm
        .preparar_envio(&admin, matriz, exp, &[caja16.id], &[])
        .await
        .unwrap();
    alm.marcar_enviado(&admin, envio2.id).await.unwrap();
    alm.recibir_envio(&admin, envio2.id, &[caja16.id])
        .await
        .unwrap();
    alm.ajustar(&admin, pieza, exp, Cantidad::Unidades(5), "merma en conteo")
        .await
        .unwrap();
    let devolucion2 = alm
        .preparar_envio(&admin, exp, matriz, &[caja16.id], &[])
        .await
        .unwrap();
    assert!(alm.marcar_enviado(&admin, devolucion2.id).await.is_err());
    // Nada cambió: sigue preparado y el saldo intacto.
    let (doc, _) = alm
        .envio_por_folio(&admin, devolucion2.folio.get())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(doc.estado, EstadoEnvio::Preparado);
    assert_eq!(
        alm.existencia(&admin, pieza, exp).await.unwrap(),
        Cantidad::Unidades(5)
    );
}

// -------- 3.4 Discrepancia: faltantes extraviados y notificados --------

#[tokio::test]
async fn discrepancia_extraviada_y_notificada() {
    let (alm, admin, matriz) = con_admin().await;
    let exp = expendio(&alm, &admin, "SNA").await;
    let peso = producto_peso(&alm, &admin, "pechuga").await;

    let sueltas = alm
        .etiquetar_pesadas(
            &admin,
            peso,
            matriz,
            &[Gramos::new(900), Gramos::new(1100), Gramos::new(500)],
        )
        .await
        .unwrap();
    let envio = alm
        .preparar_envio(
            &admin,
            matriz,
            exp,
            &[],
            &[sueltas[0].id, sueltas[1].id, sueltas[2].id],
        )
        .await
        .unwrap();
    alm.marcar_enviado(&admin, envio.id).await.unwrap();

    // Solo llegan dos de tres.
    let recibido = alm
        .recibir_envio(&admin, envio.id, &[sueltas[0].id, sueltas[1].id])
        .await
        .unwrap();
    assert!(recibido.discrepancia.as_deref().unwrap().contains("1 de 3"));

    // El stock cuenta solo lo presente; la faltante quedó extraviada.
    assert_eq!(
        alm.existencia(&admin, peso, exp).await.unwrap(),
        Cantidad::Gramos(Gramos::new(2000))
    );
    let perdida = alm
        .etiqueta_por_codigo(&admin, sueltas[2].codigo.get())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(perdida.estado, EstadoEtiqueta::Extraviada);

    // La administración se entera; el expendio no recibe ese aviso.
    let bandeja = alm.bandeja(&admin, matriz).await.unwrap();
    assert_eq!(bandeja.len(), 1);
    assert!(bandeja[0].mensaje.contains(envio.folio.get()));
    assert!(alm.bandeja(&admin, exp).await.unwrap().is_empty());

    // Lo extraviado tampoco alerta por caducidad.
    let sistema = ContextoAcceso::sistema();
    let cerca = instante_antes_de(sueltas[2].caducidad, 3);
    let _ = alm.barrer_por_vencer(&sistema, cerca).await.unwrap();
    let avisos = alm.bandeja(&admin, exp).await.unwrap();
    // el aviso del expendio (si lo hay) solo cubre las 2 presentes
    assert!(avisos.iter().all(|n| !n.mensaje.contains("3 etiquetas")));

    // Una recepción completa no genera ruido.
    let otra = alm
        .etiquetar_pesadas(&admin, peso, matriz, &[Gramos::new(600)])
        .await
        .unwrap();
    let envio2 = alm
        .preparar_envio(&admin, matriz, exp, &[], &[otra[0].id])
        .await
        .unwrap();
    alm.marcar_enviado(&admin, envio2.id).await.unwrap();
    let recibido2 = alm
        .recibir_envio(&admin, envio2.id, &[otra[0].id])
        .await
        .unwrap();
    assert!(recibido2.discrepancia.is_none());
    // Sigue habiendo UNA sola notificación de discrepancia (la del primer envío).
    let discrepancias = alm
        .bandeja(&admin, matriz)
        .await
        .unwrap()
        .into_iter()
        .filter(|n| n.tipo == domain::notificacion::TipoNotificacion::DiscrepanciaEnvio)
        .count();
    assert_eq!(discrepancias, 1);
}

// -------- 3.4 Contenido comprometido, cancelación y direcciones --------

#[tokio::test]
async fn contenido_comprometido_cancelacion_y_direcciones() {
    let (alm, admin, matriz) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let b = expendio(&alm, &admin, "SNB").await;
    let pieza = producto_pieza(&alm, &admin, "queso 500 g").await;
    let peso = producto_peso(&alm, &admin, "tripa").await;

    // Envío vacío y traspaso lateral se rechazan.
    assert!(
        alm.preparar_envio(&admin, matriz, a, &[], &[])
            .await
            .is_err()
    );
    let caja = alm
        .cerrar_caja_pieza(&admin, pieza, matriz, 10)
        .await
        .unwrap();
    assert!(
        alm.preparar_envio(&admin, a, b, &[caja.id], &[])
            .await
            .is_err()
    );

    // Una caja comprometida no entra en un segundo envío.
    let envio = alm
        .preparar_envio(&admin, matriz, a, &[caja.id], &[])
        .await
        .unwrap();
    assert!(
        alm.preparar_envio(&admin, matriz, b, &[caja.id], &[])
            .await
            .is_err()
    );

    // Una etiqueta agrupada en caja no viaja como suelta.
    let ets = alm
        .etiquetar_pesadas(&admin, peso, matriz, &[Gramos::new(500)])
        .await
        .unwrap();
    alm.cerrar_caja_pesadas(&admin, &[ets[0].id]).await.unwrap();
    assert!(
        alm.preparar_envio(&admin, matriz, a, &[], &[ets[0].id])
            .await
            .is_err()
    );

    // Cancelar libera; el contenido puede viajar en otro envío.
    alm.cancelar_envio(&admin, envio.id).await.unwrap();
    let (doc, _) = alm
        .envio_por_folio(&admin, envio.folio.get())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(doc.estado, EstadoEnvio::Cancelado);
    assert!(
        alm.preparar_envio(&admin, matriz, b, &[caja.id], &[])
            .await
            .is_ok()
    );
}

// -------- 3.4 Permisos y alcance --------

#[tokio::test]
async fn permisos_y_alcance_en_envios() {
    let (alm, admin, matriz) = con_admin().await;
    let exp = expendio(&alm, &admin, "SNA").await;
    let pieza = producto_pieza(&alm, &admin, "queso 500 g").await;
    let caja = alm
        .cerrar_caja_pieza(&admin, pieza, matriz, 10)
        .await
        .unwrap();

    // Preparar exige `enviar` sobre el ORIGEN: quien solo opera el expendio no
    // prepara surtidos desde matriz.
    let del_expendio = usuario(
        &alm,
        &admin,
        "ana",
        &[Permiso::Enviar, Permiso::Recibir],
        exp,
    )
    .await;
    assert!(
        alm.preparar_envio(&del_expendio, matriz, exp, &[caja.id], &[])
            .await
            .is_err()
    );

    let envio = alm
        .preparar_envio(&admin, matriz, exp, &[caja.id], &[])
        .await
        .unwrap();
    alm.marcar_enviado(&admin, envio.id).await.unwrap();

    // Recibir exige `recibir` sobre el DESTINO.
    let sin_recibir = usuario(&alm, &admin, "beto", &[Permiso::Enviar], exp).await;
    assert!(
        alm.recibir_envio(&sin_recibir, envio.id, &[caja.id])
            .await
            .is_err()
    );
    assert!(
        alm.recibir_envio(&del_expendio, envio.id, &[caja.id])
            .await
            .is_ok()
    );

    // Fuera del alcance de ambos extremos, el documento ni se ve.
    let ajeno = expendio(&alm, &admin, "SNB").await;
    let de_otro_lado = usuario(&alm, &admin, "caro", &[Permiso::Enviar], ajeno).await;
    assert!(
        alm.envio_por_folio(&de_otro_lado, envio.folio.get())
            .await
            .unwrap()
            .is_none()
    );
}

// -------- 3.4 La recepción enciende la alerta; el tránsito no alerta --------

#[tokio::test]
async fn recepcion_enciende_alerta_y_transito_no_alerta() {
    let (alm, admin, matriz) = con_admin().await;
    let exp = expendio(&alm, &admin, "SNA").await;
    let peso = producto_peso(&alm, &admin, "tripa de pollo").await;
    let sistema = ContextoAcceso::sistema();

    // Surtir dos etiquetas sueltas al expendio.
    let sueltas = alm
        .etiquetar_pesadas(&admin, peso, matriz, &[Gramos::new(900), Gramos::new(1100)])
        .await
        .unwrap();
    let envio = alm
        .preparar_envio(&admin, matriz, exp, &[], &[sueltas[0].id, sueltas[1].id])
        .await
        .unwrap();
    alm.marcar_enviado(&admin, envio.id).await.unwrap();
    alm.recibir_envio(&admin, envio.id, &[sueltas[0].id, sueltas[1].id])
        .await
        .unwrap();

    // Una de las dos regresa a matriz y queda EN TRÁNSITO (enviada, no recibida).
    let devolucion = alm
        .preparar_envio(&admin, exp, matriz, &[], &[sueltas[0].id])
        .await
        .unwrap();
    alm.marcar_enviado(&admin, devolucion.id).await.unwrap();

    // El barrido cerca de la caducidad: alerta SOLO la etiqueta que sigue en el
    // expendio; la que va en camino no cuenta.
    let cerca = instante_antes_de(sueltas[1].caducidad, 3);
    assert_eq!(alm.barrer_por_vencer(&sistema, cerca).await.unwrap(), 2);
    let bandeja = alm.bandeja(&admin, exp).await.unwrap();
    assert_eq!(bandeja.len(), 1);
    assert!(bandeja[0].mensaje.contains("1 etiquetas"));
    assert!(bandeja[0].mensaje.contains("tripa de pollo"));
}
