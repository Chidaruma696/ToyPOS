## 1. Dominio: cantidad tipada por unidad (D24)

- [x] 1.1 Definir `Cantidad` tipada por unidad: `Unidades(i64)` para `pieza` y reutilizando `Gramos` (D6) para `peso_variable`, de modo que el compilador impida mezclar unidades con gramos
- [x] 1.2 Derivar/validar la unidad de una `Cantidad` contra el producto (`Producto::unidad_venta`): un movimiento con unidad discordante se rechaza
- [x] 1.3 Operaciones de `Cantidad`: suma/resta con signo dentro de la misma unidad y comparación con cero (para la regla de no-negatividad)

## 2. Dominio: existencia, movimiento y reglas (D23, D25, D26, D27)

- [x] 2.1 Modelar `TipoMovimiento` **extensible** con `Ajuste` (único construido hoy) y reservados `Recepcion` y `Venta` (los postean capacidades futuras). La reversión NO es un tipo: es el `EstadoMovimiento` `aplicado ⇄ revertido` (D15/D27)
- [x] 2.2 Modelar `MovimientoInventario { id, producto, sucursal, tipo, cantidad (con signo), motivo?, estado: aplicado|revertido, actualizado }` con `uuid` y `updated_at` (sync-ready, D12), append-only (sin borrado)
- [x] 2.3 Modelar `Existencia { producto, sucursal, cantidad }` y la función pura `aplicar(delta)` que respeta la unidad y **rechaza el saldo negativo** (D25)
- [x] 2.4 Regla de **ajuste absoluto** (D26): fijar la existencia a una **cantidad contada**; el movimiento `Ajuste` guarda el delta resultante (`objetivo − saldo`). Exige `ajustar_inventario` + alcance (`requiere_en`) y **motivo** no vacío. Sin API por delta
- [x] 2.5 Regla de **reversión** (D27, realiza D15): toggle `aplicado ⇄ revertido` del movimiento original + movimiento compensatorio que deshace el efecto; sin duplicar la entidad; la reversión que dejaría negativo se rechaza (D25)

## 3. Permisos nuevos (D28)

- [x] 3.1 Añadir al enum `Permiso` las variantes `AjustarInventario` y `VerInventario`, clasificadas **por-sucursal** en `Permiso::clase`; no toca el mecanismo de `organization-access` (RBAC extensible, D2)

## 4. Storage: esquema y repositorio (D23, D29, D30)

- [x] 4.1 Migración: tabla `existencia (producto_id, sucursal_id, cantidad, updated_at, PK(producto,sucursal))` y tabla `movimiento_inventario (id, producto_id, sucursal_id, tipo, cantidad, motivo, estado, updated_at)` append-only; índices sobre las claves calientes `(producto, sucursal)` (D17)
- [x] 4.2 Definir el trait `Inventario` (agnóstico de almacenamiento; cada op recibe `ContextoAcceso`): `ajustar`, `revertir_movimiento`, `existencia`, `movimientos_de`
- [x] 4.3 Implementar `ajustar` en el backend SQLite: en **una sola transacción** registrar el movimiento y **actualizar el saldo materializado** (D23), con enforcement de permiso+alcance (D3), auditoría (D11) y no-negatividad (D25)
- [x] 4.4 **Materialización perezosa** (D30): la existencia ausente vale 0; el primer movimiento crea su fila; el alta de producto NO crea filas de existencia
- [x] 4.5 Implementar `revertir_movimiento`: compensación + toggle de estado en una transacción, auditada; rechazar si dejaría el saldo negativo
- [x] 4.6 Implementar `existencia` (lectura O(1) del saldo) tras `ver_inventario` + alcance, y `movimientos_de` (historial del ledger) dentro del alcance

## 5. Verificación

- [x] 5.1 El saldo materializado siempre iguala la suma de los movimientos vigentes tras N operaciones (D23)
- [x] 5.2 No negatividad: una salida que excede lo disponible y un ajuste a valor negativo se rechazan sin alterar el saldo (D25)
- [x] 5.3 Reversión: ida y vuelta restaura el saldo sin duplicar la entidad; una reversión que dejaría negativo se rechaza (D27)
- [x] 5.4 Ajuste: exige `ajustar_inventario`, alcance que cubra la sucursal y motivo no vacío (D26)
- [x] 5.5 Aislamiento por alcance (D3): un usuario no consulta ni ajusta el inventario de una sucursal fuera de su alcance
- [x] 5.6 Unidad por tipo de producto (D24): la existencia de `peso_variable` va en gramos y la de `pieza` en unidades; un movimiento con unidad discordante se rechaza
- [x] 5.7 Materialización perezosa (D30): dar de alta un producto deja su existencia en 0 en toda sucursal hasta el primer movimiento
- [x] 5.8 Concurrencia (D14): N ajustes simultáneos sobre existencias sin corromper el saldo ni permitir sobregiro
- [x] 5.9 Actualizar el spec de `product-catalog` (nota de alcance) y dejar `openspec validate inventory-per-branch --strict` en verde
