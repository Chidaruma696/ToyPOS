## 1. Dominio: enums extensibles y permisos (D38–D41)

- [x] 1.1 `folio.rs`: `TipoDocumento::Envio` con letra `E`
- [x] 1.2 `acceso.rs`: permisos por-sucursal `Enviar` y `Recibir` (clase, contexto `sistema`, `TODOS_LOS_PERMISOS` de storage)
- [x] 1.3 `inventario.rs`: `TipoMovimiento::Envio`; `etiqueta.rs`: `EstadoEtiqueta::Extraviada`; `notificacion.rs`: `TipoNotificacion::DiscrepanciaEnvio` (CHECKs de SQL ampliados en 3.1)
- [x] 1.4 Pruebas de dominio de lo anterior (folio `MATE1`, clases de permiso)

## 2. Dominio: el documento de envío (D38, D39, D41, D42)

- [x] 2.1 `envio.rs`: entidad `Envio` (folio, origen, destino, estados `Preparado|Enviado|Recibido|Cancelado`, marcas de tiempo) con validación de dirección (un extremo es la matriz, origen ≠ destino) y permiso `enviar` + alcance origen al preparar
- [x] 2.2 Transiciones: enviar (desde preparado), cancelar (solo preparado), recibir (solo enviado, permiso `recibir` + alcance destino); transiciones inválidas rechazadas
- [x] 2.3 Cálculo de los deltas por producto de un contenido (etiquetas → gramos, cajas de pieza → unidades) para los movimientos de 3.2
- [x] 2.4 Pruebas de dominio: direcciones, ciclo de estados, permisos, deltas por producto

## 3. Storage: esquema, repo y efectos (D40, D41, D42, D43)

- [x] 3.1 Migraciones: tabla `envio` idempotente; columnas aditivas `envio_id` en `etiqueta` y `caja_etiquetado`; CHECKs ampliados (`Extraviada`, `Envio`, `DiscrepanciaEnvio`) — atención: los CHECK existentes viven en `CREATE TABLE IF NOT EXISTS`, definir la estrategia para bases ya creadas
- [x] 3.2 Repo `Envios`: `preparar` (valida y liga contenido, folio en la misma tx), `enviar` (devolución: movimiento `envio` por producto con no-negatividad D25), `cancelar` (libera contenido), `recibir` (presentes → destino; faltantes → `Extraviada`; movimientos `recepcion` por producto solo si el destino es expendio; discrepancia anotada y notificada a la bandeja de la matriz); todo auditado (D11) y cada operación en una sola transacción
- [x] 3.3 Lectura: envío por folio con su contenido (presentes/faltantes consultables), dentro del alcance
- [x] 3.4 Pruebas de integración: surtido completo (stock nace al recibir, matriz sin movimientos), devolución (descuenta al enviar, no-negatividad), discrepancia (faltante extraviado + notificación; recepción completa sin ruido), contenido comprometido/liberado, permisos/alcance, y la alerta "por vencer" encendida por una recepción

## 4. Verificación

- [x] 4.1 `cargo fmt --check`, `cargo clippy -D warnings` y `cargo test --workspace` en verde
- [x] 4.2 `openspec validate --all` en verde y tasks.md al día
