## Why

ToyPOS es un sistema de punto de venta multi-sucursal para una operación **matriz → expendios afiliados**: la matriz produce, empaqueta y gobierna; los expendios reciben y venden. Antes de construir las funciones visibles (etiquetado con báscula, traspasos, checkout, sync, panel admin) hay que asentar el **núcleo del que todas dependen**: quién es cada quién y qué puede hacer/ver, qué es un producto, cómo se le pone precio y cómo se controla el efectivo. Si estos cimientos nacen torcidos (roles cerrados, precio único, aislamiento por disciplina en vez de por diseño), cada capa posterior hereda el defecto. Este cambio establece esos cimientos y nada más.

## What Changes

- **Organización + acceso**: se introducen las **sucursales** (tipo `matriz` o `expendio`), los **usuarios**, y un **RBAC modular**: un catálogo de **permisos atómicos** que se componen en **roles a medida** (no roles cerrados/hardcodeados). Cada asignación de rol lleva un **alcance** (`global` | `matriz` | una `sucursal`) — el alcance es el **mecanismo de aislamiento**: un usuario de un expendio no puede leer ni tocar datos de otro porque su alcance no los alcanza.
- **Catálogo de productos tipado**: cada producto es de naturaleza **peso-variable** (producido en matriz, se vende por kg) o **pieza** (comprado a proveedor externo, trae su barcode de fábrica, se vende por unidad). El tipo condiciona etiquetado, venta y stock aguas abajo.
- **Precios por sucursal y nivel**: el precio es función de `producto × sucursal × nivel {menudeo, medio-mayoreo, mayoreo}`. Los fija **directamente el admin** (no pasan por autorización). Incluye **copiar precios** de una sucursal a otra (para arrancar una sucursal nueva sin recapturar) y edición independiente por sucursal. Un producto **sin precio en su sucursal queda CONGELADO**: existe en inventario pero no se puede vender hasta que el admin le ponga precio.
- **Subsistema genérico de autorizaciones**: un mecanismo único **solicitud → aprobada/rechazada** parametrizado por tipo, extensible. En esta fundación su primer consumidor son los **gastos**; el tipo `traspaso` se conectará cuando aterrice esa capacidad. Los **precios NO** usan este subsistema (son edición directa del admin).
- **Gestión de efectivo**: registro de **gastos** (todos requieren autorización) y **corte de caja diario**. Regla: el gasto **autorizado** resta del efectivo esperado; el gasto **no autorizado** se **cobra al cajero** (se convierte en su faltante).

## Capabilities

### New Capabilities
- `organization-access`: Sucursales (matriz/expendio), usuarios, permisos atómicos, roles componibles a medida, y enforcement de permiso + alcance (el alcance = aislamiento entre sucursales).
- `product-catalog`: Producto tipado (peso-variable/producido vs pieza/comprado-externo con barcode de fábrica) y sus atributos base.
- `pricing`: Precio por `producto × sucursal × nivel`; copiar precios entre sucursales; edición por sucursal; estado **congelado** cuando falta precio.
- `authorization-workflow`: Subsistema genérico de solicitudes con estados (pendiente → aprobada/rechazada), aprobador determinado por permiso, extensible por tipo; primer consumidor: gastos.
- `cash-management`: Registro de gastos (sujetos a autorización) y corte de caja diario, con la regla de cargo al cajero por gasto no autorizado.

### Modified Capabilities
<!-- Ninguna: es un proyecto nuevo, no hay specs existentes que modificar. -->

## Impact

- **Repo nuevo y vacío**: esta es la primera capa de código. Sin migraciones ni breaking changes.
- **Stack fijado** (decidido en exploración, se detalla en `design.md`): backend **Rust + Tokio**; front **Tauri + Vue**; **Windows-primario** cross-platform (por drivers de impresora); modelo de conectividad **online con degradación offline** (la ruta caliente resuelve local y sincroniza en segundo plano).
- **Fuera de alcance** (cambios posteriores que dependen de esta fundación): etiquetado báscula/EAN-13 con discriminador antiduplicado, cajas y traspaso físico, recepción con validación de completitud, checkout/POS, sincronización offline, panel de estadísticas del admin.
- **Decisión abierta a confirmar en `design.md`**: selección de nivel en caja. Default propuesto: `mayoreo` = venta por caja/granel; `menudeo ↔ medio-mayoreo` por umbral de cantidad/peso automático, con override manual si el usuario tiene el permiso.
