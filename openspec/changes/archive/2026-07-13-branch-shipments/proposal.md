## Why

El sistema ya etiqueta la producción (identidad por etiqueta, caducidad) y lleva inventario por sucursal, pero **la mercancía no puede viajar**: no hay cómo surtir un expendio ni devolver a matriz. El envío es la capa que conecta ambas — hace nacer el stock del expendio (movimiento `recepcion`, reservado desde `inventory`) y **enciende la alerta de caducidad** que quedó latente en `labeling-expiry` (solo alerta stock en expendios, y hoy nada llega a ellos). Las decisiones de diseño ya están cerradas con el negocio.

## What Changes

- **Nueva capacidad `shipments`**: el **envío** como documento citable con folio (D18), con estados mínimos `preparado → enviado → recibido` — **sin flujo de aprobación** (D4: el traspaso es un envío, matriz decide y manda) — y cancelable antes de enviar.
  - **Direcciones**: matriz→expendio (surtido) y expendio→matriz (devolución). Un extremo siempre es la matriz; **sin traspasos expendio→expendio**.
  - **Contenido**: cajas cerradas y etiquetas sueltas del etiquetado, de la sucursal origen, congeladas en el documento al preparar; cancelar las libera.
  - **Efectos de inventario asimétricos** (consecuencia de D34, matriz sin stock propio): enviar desde un **expendio** postea la salida (`envio`); recibir en un **expendio** postea la entrada (`recepcion`); la matriz jamás postea movimientos.
  - **Recepción de lo real**: el receptor confirma lo que llegó; los faltantes quedan **extraviados** (ni en origen ni en destino, fuera del stock y de las alertas), anotados en el documento, y disparan una **notificación de discrepancia** a la administración (reusa `notifications`).
  - **Las etiquetas viajan**: al recibir, etiquetas y cajas cambian a la sucursal destino conservando identidad y caducidad — con eso el barrido "por vencer" las encuentra.
  - Permisos nuevos por-sucursal: `enviar` (preparar/enviar/cancelar, sobre el origen) y `recibir` (sobre el destino).
- **`inventory` modificada**: el tipo de movimiento `recepcion` deja de estar reservado (lo postea la recepción) y nace el tipo `envio` (salida por devolución); solo `venta` queda reservado para el POS.
- **Fuera de alcance**: el transporte físico del payload y el "envío carga su propio catálogo" (capa de **sync** futura — aquí ambos extremos operan sobre la base local sync-ready, D12); la verificación etiqueta-por-etiqueta dentro de una caja recibida (llega con el escaneo del POS); reversión de movimientos `envio`/`recepcion` (la corrección de saldo usa el ajuste absoluto D26); pedidos/surtido automático.

## Capabilities

### New Capabilities
- `shipments`: el envío entre sucursales — documento con folio y estados, contenido de cajas/etiquetas, efectos de inventario asimétricos, recepción de lo real con discrepancia notificada, y el viaje de las etiquetas.

### Modified Capabilities
- `inventory`: la lista de tipos de movimiento cambia — `recepcion` se activa (lo postea la recepción del envío) y se agrega `envio` (salida); `venta` sigue reservado.

## Impact

- **`domain`**: módulo nuevo `envio` (documento, estados, direcciones, validaciones de contenido); `folio.rs` gana `TipoDocumento::Envio`; `acceso.rs` gana `enviar`/`recibir`; `inventario.rs` gana `TipoMovimiento::Envio`; `etiqueta.rs` gana el estado `Extraviada`; `notificacion.rs` gana el tipo `DiscrepanciaEnvio`.
- **`storage`**: tabla `envio` y columnas aditivas `envio_id` en `etiqueta` y `caja_etiquetado`; repo `Envios` tras el punto único de acceso (movimientos y saldos en la misma transacción, D23; auditoría D11); la discrepancia emite por el mecanismo de `notifications`.
- **Sin breaking changes**: capa aditiva; los enums extensibles (permisos, tipos de movimiento, estados, notificaciones) crecen sin tocar variantes existentes.
