## Why

La fundación dejó un **catálogo global** de productos pero sin **existencias**: hoy el sistema sabe *qué* productos hay, no *cuántos* hay en cada sucursal. Sin inventario por sucursal no se puede recibir mercancía, ni impedir vender lo que no hay, ni alimentar el reabasto automático; y el motivo original de la auditoría —"alguien desajusta el inventario y no hay forma de probar quién fue" (D11)— sigue sin su objeto. Esta capa introduce la existencia por sucursal como un **ledger de movimientos** atribuible y reversible, sobre el que se apoyarán recepción, POS, reabasto y reportes.

## What Changes

- **Existencia por `producto × sucursal`**: cada sucursal lleva su propia cantidad de cada producto del catálogo global, en la **unidad natural del producto** (D6): `pieza` en unidades enteras, `peso_variable` en gramos. La ausencia de existencia es 0 (no se materializan filas vacías).
- **Ledger de movimientos tipados**: toda variación de existencia es un **movimiento** append-only (tipo, cantidad con signo, motivo, actor, marca de tiempo) del que se deriva el saldo; el saldo se mantiene materializado por `(producto, sucursal)` para lectura instantánea en la ruta caliente (D17). El movimiento es el dato de dominio (*qué/por qué*) que complementa la bitácora transversal (*quién*, D11).
- **Ajuste manual de existencias**: único escritor real hoy —alta inicial de stock y correcciones— protegido por un permiso nuevo `ajustar_inventario` + alcance, con **motivo obligatorio**. Los tipos `Recepcion` y `Venta` quedan definidos en el enum pero los postean sus capacidades futuras (recepción, POS); no se construyen aquí (YAGNI).
- **Stock no negativo**: una salida o ajuste que dejaría la existencia por debajo de cero se **rechaza**; el saldo nunca miente con negativos.
- **Reversión de movimientos**: realiza el principio de reversión de la fundación (D15) para inventario —un movimiento lleva estado `aplicado ⇄ revertido`, revertir postea un movimiento compensatorio que restaura el saldo sin borrar nada, auditado—.
- **Consulta de existencia/disponible**: expuesta a los consumidores futuros (POS) tras `ver_inventario` + alcance.

## Capabilities

### New Capabilities
- `inventory`: Existencia por `producto × sucursal` sobre el catálogo global, como ledger de **movimientos** tipados (append-only, auditados, reversibles D15) con **saldo materializado** para lectura O(1); ajuste manual con motivo; invariante de **no negatividad**; unidad de cantidad derivada del tipo de producto (D6); aislamiento por alcance (D3). Nuevos permisos atómicos `ajustar_inventario` y `ver_inventario` (por-sucursal).

### Modified Capabilities
- `product-catalog`: se actualiza la nota de alcance del requisito "Catálogo de productos global gobernado por matriz" —el **inventario ya no es una capacidad futura**: vive en la capacidad `inventory`—. El comportamiento del catálogo (global, `gestionar_productos`) no cambia.

## Impact

- **Código**: nuevas piezas en el crate `domain` (cantidad tipada, movimiento de inventario, reglas de saldo/no-negatividad/reversión) y en `storage` (tablas `existencia` y `movimiento_inventario` sync-ready, trait `Inventario` + backend SQLite con enforcement de permiso/alcance/auditoría/no-negatividad). Sin cambios en las caras (aún no existen).
- **Catálogo de permisos**: dos permisos atómicos nuevos, por-sucursal. El RBAC es extensible por diseño (D2), así que **no** modifica el mecanismo de `organization-access`.
- **Esquema sync-ready** (D12): las nuevas entidades nacen con `uuid`, `updated_at` y sin borrado físico; el ledger es append-only por naturaleza. El motor de sync sigue siendo un cambio futuro.
- **Fuera de alcance** (dependen de esta capa): recepción con validación de completitud y postado de movimientos `Recepcion`; POS y el descuento de stock (`Venta`) más la decisión de "no vender sin stock"; **devoluciones** (cliente → tienda, tienda → matriz, o mercancía no vendida directa a matriz), que se montarán sobre la **reversión** (D15/D27) y los movimientos de esta capa; reabasto automático (usa el historial de movimientos como estadística de rotación); reportes de existencias y de más vendidos; **punto de reorden / stock mínimo** (config que consumirá el reabasto, por `producto × sucursal`). Nada de esto se construye aquí.
- **No rompe nada**: es una capa aditiva sobre la fundación ya archivada; sin migraciones destructivas.
