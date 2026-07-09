## 1. Scaffolding del workspace

- [ ] 1.1 Inicializar repositorio git y `.gitignore` para Rust/Node
- [ ] 1.2 Crear workspace de Cargo con crate `domain` (lógica pura, sin I/O) y crate `storage` (persistencia)
- [ ] 1.3 Añadir dependencias base: `tokio`, `sqlx`/`rusqlite` (SQLite), `serde`, `thiserror`, `uuid`
- [ ] 1.4 Definir tipos de dinero (`Centavos`) y peso (`Gramos`) como enteros con envoltorios newtype (D6)
- [ ] 1.5 Configurar CI mínimo: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`

## 2. Núcleo de acceso a datos y contexto (D3, D7)

- [ ] 2.1 Definir el `ContextoAcceso { actor, permisos_efectivos, alcance }`
- [ ] 2.2 Definir repositorios como traits agnósticos de almacenamiento; toda operación recibe `ContextoAcceso`
- [ ] 2.3 Implementar el filtrado por alcance por defecto en la capa de repositorio (rechazar/filtrar si el alcance no cubre el dato)
- [ ] 2.4 Implementar backend SQLite de los repositorios y migraciones iniciales
- [ ] 2.5 Utilidad de verificación de permisos (`requiere(permiso)`) reutilizable por todos los casos de uso

## 3. Organización y acceso (`organization-access`)

- [ ] 3.1 Modelar `Sucursal { tipo: matriz|expendio }` con la regla de matriz única
- [ ] 3.2 Modelar `Usuario`, credenciales y sesión con permisos efectivos (unión de roles)
- [ ] 3.3 Definir el catálogo de **permisos atómicos** como enumeración extensible
- [ ] 3.4 Modelar `Rol` como conjunto editable de permisos; alta/edición en caliente
- [ ] 3.5 Modelar la asignación rol→usuario con `Alcance { global | matriz | conjunto de sucursales }` y calcular el alcance efectivo (unión de asignaciones)
- [ ] 3.6 Garantizar que ningún usuario alcance una sucursal no asignada (empleado de 1, de varias, o admin global)
- [ ] 3.7 Implementar autenticación (login válido/ inválido) y derivación de permisos efectivos
- [ ] 3.8 Proteger gestión de usuarios/roles/sucursales con sus permisos (`gestionar_*`)

## 4. Catálogo de productos (`product-catalog`)

- [ ] 4.1 Modelar `Producto { tipo: peso_variable|pieza, nombre, unidad, activo }` con tipo inmutable
- [ ] 4.2 Derivar unidad de venta del tipo (peso→kg, pieza→unidad)
- [ ] 4.3 Almacenar GTIN en productos `pieza` con unicidad global; prohibir GTIN en `peso_variable`
- [ ] 4.4 Regla de producto inactivo (no vendible, conserva historial)

## 5. Precios (`pricing`)

- [ ] 5.1 Modelar precio con clave `(producto, sucursal, nivel)` y niveles `menudeo|medio_mayoreo|mayoreo`
- [ ] 5.2 Caso de uso de edición directa de precio protegido por `editar_precio` (aplica inmediato)
- [ ] 5.3 Implementar **copiar precios** origen→destino (crea/actualiza destino, no toca origen)
- [ ] 5.4 Implementar el estado **congelado** como derivado (ausencia de precio de menudeo vigente)
- [ ] 5.5 Exponer consulta "¿producto vendible en sucursal?" para consumidores futuros (POS)

## 6. Subsistema de autorizaciones (`authorization-workflow`)

- [ ] 6.1 Modelar `SolicitudAutorizacion { tipo, solicitante, alcance, payload, estado, resolutor, resuelto_en }`
- [ ] 6.2 Implementar la máquina de estados `pendiente → aprobada|rechazada` con estados terminales inmutables
- [ ] 6.3 Exigir en la resolución `autorizar_<tipo>` + alcance que cubra la solicitud
- [ ] 6.4 Definir el contrato (trait) de **efecto delegado al tipo** que el subsistema invoca al aprobar
- [ ] 6.5 Registrar autoría y marca de tiempo en cada resolución

## 7. Gestión de efectivo (`cash-management`)

- [ ] 7.1 Modelar `Gasto { monto, concepto, cajero, sucursal, estado }` que nace no autorizado
- [ ] 7.2 Registrar gasto creando una solicitud `gasto` (consumidor del subsistema de 6.4)
- [ ] 7.3 Implementar el **corte de caja** diario: esperado = fondo + entradas − gastos autorizados
- [ ] 7.4 Calcular diferencia contra conteo físico (faltante a cargo del cajero)
- [ ] 7.5 Implementar cargo al cajero por gasto rechazado
- [ ] 7.6 Implementar la **ventana de gracia** de gastos pendientes (default: corte del día siguiente)

## 8. Verificación

- [ ] 8.1 Pruebas de aislamiento por alcance (un expendio no lee/escribe datos de otro)
- [ ] 8.2 Pruebas de RBAC (permiso concede exactamente su capacidad; agregar permiso en caliente)
- [ ] 8.3 Pruebas de precios (por sucursal independiente, copiar, congelado/descongelado)
- [ ] 8.4 Pruebas del ciclo de solicitud (estados terminales, permiso+alcance del aprobador)
- [ ] 8.5 Pruebas del corte de caja (solo autorizados restan; rechazado y pendiente-vencido se cobran)
- [ ] 8.6 `openspec validate foundation-core --strict` en verde
