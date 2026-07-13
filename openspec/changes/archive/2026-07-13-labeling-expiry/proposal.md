## Why

La fundación dejó **diferido a etiquetado** (D22) justo lo que la operación necesita a diario: la **etiqueta de báscula** por ítem de los `peso_variable` (peso + discriminador antiduplicado) y la impresión física de etiquetas. Además el negocio quiere dar al cliente una **fecha de caducidad** nominal (el producto va congelado y al vacío; la fecha es de tranquilidad, no de bloqueo) y enterarse **a tiempo** del stock por vencer en los expendios. Hoy nada modela etiquetas, vida útil ni avisos; esta capa es el prerrequisito acordado antes del envío entre sucursales (la mercancía viaja ya etiquetada).

## What Changes

- **Vida útil por producto** en el catálogo: `vida_util` con default de **9 meses**, editable por producto. La caducidad se calcula `fecha_etiquetado + vida_util`.
- **Nueva capacidad `labeling`**: el acto de **etiquetar** producción en matriz.
  - `peso_variable`: cada pesada produce una **etiqueta por-ítem** con identidad propia — barcode EAN-13 per-ítem que codifica **peso + discriminador antiduplicado** (clase de código distinta del barcode estable de producto, D22).
  - `pieza`: no se pesa ni lleva etiqueta por-ítem; usa su barcode estable de producto y solo se registra la **cantidad por caja** (etiqueta de caja).
  - **Contenido físico de la etiqueta, exactamente 3 elementos**: barcode, nombre del producto (hasta 2 líneas) y caducidad impresa `DD/MM/AAAA`. **Sin precio** (D8) y **sin fecha de etiquetado**.
  - La `fecha_etiquetado` se guarda **internamente** y **nunca se imprime**.
  - La caducidad es **nominal e informativa**: **jamás bloquea la venta**, ni siquiera vencida. Sirve para rotación FIFO.
  - Permiso nuevo por-sucursal: `etiquetar`.
  - **Alerta "por vencer"**: automática (nadie la dispara a mano), **5 días antes** de caducar, solo stock **no vendido** y **en un expendio** (no matriz), **agrupada por producto**, notificando a **ambos**: administración y la sucursal afectada.
- **Nueva capacidad `notifications`**: mecanismo de **notificaciones del sistema a usuarios** (distinto del subsistema de autorizaciones, D4): bandeja por destino (usuario o sucursal), no bloqueante, con marcar-leída. El POS del expendio también tendrá bandeja, no solo el admin. Primer productor: la alerta "por vencer".
- **Fuera de alcance**: la pantalla de etiquetado (el diseño Kanso ya está decidido y se documenta en design.md, pero la capa UI aún no existe); la impresión física (driver de impresora); el inventario de matriz — **matriz produce sin llevar stock propio** (decisión de negocio, ver design D34): el stock del sistema nace con la recepción en los expendios — y el viaje de etiquetas (**envío/recepción**, capa siguiente); venta y estado "vendida" real de la etiqueta (POS futuro).

## Capabilities

### New Capabilities
- `labeling`: etiquetado de producción en matriz — etiquetas por-ítem con peso + discriminador para `peso_variable`, cantidad por caja para `pieza`, contenido físico de la etiqueta, fecha de etiquetado interna, caducidad nominal calculada y alerta "por vencer".
- `notifications`: notificaciones del sistema a usuarios — bandeja por destino (usuario o sucursal), no bloqueante, marcar-leída, auditable.

### Modified Capabilities
- `product-catalog`: cada producto gana una **vida útil** (default 9 meses) que determina su caducidad al etiquetar.

## Impact

- **`domain`**: módulo nuevo `etiqueta` (etiqueta por-ítem, discriminador, caducidad) y `notificacion`; `barcode.rs` gana la generación del EAN-13 per-ítem (prefijo interno propio, distinto del de producto); `producto.rs` gana `vida_util`; `acceso.rs` gana el permiso `etiquetar`.
- **`storage`**: tablas nuevas (`etiqueta`, `notificacion`) idempotentes; repos nuevos tras el punto único de acceso (D3/D11); la alerta "por vencer" corre como barrido del sistema (actor `sistema`, D11).
- **Sin breaking changes**: capa aditiva; `vida_util` con default no altera productos existentes.
