## 1. Scaffolding del workspace

- [x] 1.1 Inicializar repositorio git y `.gitignore` para Rust/Node
- [x] 1.2 Crear workspace de Cargo con crate `domain` (lógica pura, sin I/O) y crate `storage` (persistencia)
- [x] 1.3 Añadir dependencias base: `tokio`, `sqlx`/`rusqlite` (SQLite), `serde`, `thiserror`, `uuid`
- [x] 1.4 Definir newtypes enteros `Centavos` (dinero, 2 dec) y `Gramos` (peso, 3 dec de kg, Torrey), con helper de importe por peso (`precio/kg × g ÷ 1000`, redondeo a centavos medio-arriba) (D6)
- [x] 1.5 Configurar CI mínimo: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`

## 2. Núcleo de acceso a datos y contexto (D3, D7)

- [x] 2.1 Definir el `ContextoAcceso { actor, permisos (bolsa plana), alcance (conjunto de sucursales del usuario) }`; el `actor` es **usuario | sistema** (el `sistema` cubre bootstrap y operaciones automáticas futuras) (D3)
- [x] 2.2 Definir repositorios como traits agnósticos de almacenamiento; toda operación recibe `ContextoAcceso`
- [x] 2.3 Implementar el filtrado por alcance por defecto en la capa de repositorio (rechazar/filtrar si el alcance no cubre el dato)
- [x] 2.4 Implementar backend SQLite de los repositorios y migraciones iniciales
- [x] 2.5 Utilidad de verificación de permisos reutilizable: `requiere(permiso, sucursal)` para permisos por-sucursal y `requiere(permiso)` para de-sistema
- [x] 2.6 Registrar en **bitácora append-only e inmutable** toda operación mutante (C/U/D) en la capa de repositorio: actor (usuario o `sistema`), entidad, antes→después, alcance, timestamp (D11)
- [x] 2.7 Adoptar convenciones **sync-ready** en todas las entidades: `uuid` PK, `updated_at`, borrado lógico (sin hard delete) (D12)
- [x] 2.8 Implementar el **generador de folios** para documentos: `{código_sucursal}{tipo}{consecutivo}` todo junto, sin separadores (p. ej. `SNJC56`), secuencial por `(sucursal, tipo)`, generado local, asignado al confirmar el documento, **sin reutilización** (cancelado/revertido conserva su folio); el `uuid` (2.7) sigue siendo la identidad técnica aparte del folio (D18)
- [x] 2.9 **Convención de tiempo**: toda marca se guarda como **instante UTC capturado en el nodo** (el central nunca la reasigna/reescribe); helper de conversión UTC→zona IANA de la sucursal para presentación y para el "día" local de reportes; nada de offset fijo hardcodeado (D21)

## 3. Organización y acceso (`organization-access`)

- [x] 3.1 Modelar `Sucursal { tipo: matriz|expendio, código, zona_horaria }` con la regla de matriz única; el **código** es alfanumérico corto, **único e inmutable**, asignado al alta (no derivado del nombre), y prefija los folios de la sucursal (D18); la **zona_horaria** es un identificador IANA (default `America/Mexico_City`) para hora local y "día" de reportes (D21)
- [x] 3.2 Modelar `Usuario` con credencial **usuario + contraseña hasheada** (nunca texto plano); sesión con permisos efectivos (unión de roles) y alcance
- [x] 3.3 Definir el catálogo de **permisos atómicos** (nivel capacidad) como enumeración extensible, cada uno clasificado **por-sucursal** o **de-sistema**
- [x] 3.4 Modelar `Rol` como conjunto editable de permisos; alta/edición en caliente
- [x] 3.5 Modelar el **alcance del usuario** = conjunto de sucursales (una | varias | todas=matriz), independiente de roles/permisos; el chequeo por-sucursal es `tiene(permiso) ∧ alcance cubre la sucursal` (D3)
- [x] 3.6 Garantizar que ningún usuario alcance una sucursal no asignada (empleado de 1, de varias, o admin global)
- [x] 3.7 Implementar autenticación **local** (offline-ok): login válido/inválido, deriva el `ContextoAcceso`; sesión con **cierre por inactividad** (logout completo, sin PIN) (D16)
- [x] 3.8 Proteger gestión de usuarios/roles/sucursales con sus permisos (`gestionar_*`)
- [x] 3.9 **Sucursal activa dentro del alcance**: alcance de 1 → activa automática; varias/todas → selección/filtro en la app (default matriz); nunca amplía el alcance (D3)
- [x] 3.10 **Bootstrap del sistema**: crea la matriz + un admin inicial (alcance todas + permisos de-sistema); el mecanismo concreto se define en implementación (#13)

## 4. Catálogo de productos (`product-catalog`)

- [x] 4.1 Modelar `Producto { tipo: peso_variable|pieza, origen: matriz|externo, nombre, unidad, activo, peso_empaque? }` con **tipo y origen inmutables**; peso fijo = `pieza` de origen `matriz` con `peso_empaque` informativo (no afecta el precio) (D22)
- [x] 4.2 Derivar unidad de venta del tipo (peso→kg, pieza→unidad)
- [x] 4.3 Código de barras de `pieza`: origen `externo` (GTIN de fábrica) o `matriz` (generado); **único global** venga externo o generado; prohibirlo en `peso_variable` (se etiqueta por peso, futuro) (D22)
- [x] 4.4 Regla de producto inactivo (no vendible, conserva historial)
- [x] 4.5 Catálogo **global** (matriz da de alta); crear/editar producto requiere el permiso de-sistema `gestionar_productos` (el inventario por sucursal es capacidad futura)
- [x] 4.6 Implementar el **generador de barcodes de matriz**: **un** EAN-13 decodable (dígito verificador) **por producto (SKU)** de `pieza` origen `matriz`, con prefijo interno que no colisiona con GTIN externos ni con otros productos; todas las unidades comparten ese mismo código (impreso en el empaque, como un producto comercial); se asigna al alta (D22). La **impresión** de la etiqueta es capacidad futura

## 5. Precios (`pricing`)

- [x] 5.1 Modelar precio con clave `(producto, sucursal, nivel)` y niveles `menudeo|medio_mayoreo|mayoreo`
- [x] 5.2 Caso de uso de edición directa de precio protegido por `editar_precio` (aplica inmediato)
- [x] 5.3 Implementar **copiar precios** origen→destino (crea/actualiza destino, no toca origen)
- [x] 5.4 Implementar el estado **congelado** como derivado (ausencia de precio de menudeo vigente)
- [x] 5.5 Exponer consulta `vendible(producto, sucursal, nivel)` (hay precio vigente para ese nivel; piso menudeo) para consumidores futuros (POS) (#17)

## 6. Aprobación de gastos (`authorization-workflow`)

- [x] 6.1 Modelar `Aprobacion { gasto, alcance, estado, resolutor, resuelto_en, motivo }`
- [x] 6.2 Implementar la máquina de estados `pendiente → aprobada|rechazada|cancelada` con estados terminales inmutables (pieza reutilizable, DRY)
- [x] 6.3 Exigir en la resolución `autorizar_gasto` + alcance que cubra la sucursal del gasto; `autorizar_gasto` es **autoridad administrativa** — el rol de cajero no lo porta, así que el cajero nunca autoriza (ni propio ni ajeno)
- [x] 6.4 Aplicar el efecto sobre la **sesión de origen** del gasto: aprobado → autorizado (resta en su corte); rechazado o pendiente → no autorizado (no resta; aparece como faltante); cancelado → gasto anulado (no cuenta). Si el corte de esa sesión **ya cerró**, no se reescribe: la resolución se **liga** al gasto con su fecha (conciliación posterior, D19)
- [x] 6.5 Registrar autoría y marca de tiempo en cada resolución
- [x] 6.6 **Segregación de deberes**: el solicitante no puede resolver su propia solicitud, salvo un admin con permiso de auto-aprobación (#11)
- [x] 6.7 Rechazo y **cancelación** con **motivo** obligatorio; la cancelación (gasto por error) la resuelve un aprobador, no el solicitante (#12)

## 7. Gestión de efectivo (`cash-management`)

- [x] 7.1 Modelar `Gasto { folio, monto, concepto, cajero, sucursal, estado }` que nace no autorizado; el `folio` se emite con el generador (2.8), tipo `G`
- [x] 7.2 Registrar gasto creando su aprobación pendiente (consumidor de 6.4)
- [x] 7.3 Modelar `SesiónDeCaja { sucursal, cajero, fondo_apertura, abierta_en, efectivo_contado, cerrada_en }`: **una caja por sucursal** (un cajero = una caja); la sesión pertenece a la sucursal y registra al cajero de turno; es la frontera del "día" (D13)
- [x] 7.4 Implementar la **apertura**: el cajero declara su fondo (autodeclarado, sin arrastre entre días); **a lo sumo una sesión abierta por sucursal** — rechazar apertura si hay una sin cerrar (cerrar antes de abrir); cada sucursal independiente; días sin labor no dejan sesión pendiente (D13)
- [x] 7.5 Implementar el **cierre (corte)**: emitir el **folio** del corte (generador 2.8, tipo `C`); clasificar ventas por método de pago; esperado = fondo + ventas efectivo − gastos autorizados; venta neta = ventas efectivo (fondo aparte); transferencia/depósito informativos
- [x] 7.6 Implementar el **cierre a ciegas**: el cajero captura solo su efectivo contado; ver la conciliación (esperado/diferencia) es permiso del administrador
- [x] 7.7 Calcular y **mostrar** la diferencia esperado vs contado, **sin sanción automática** (el admin decide); importes exactos, sin redondeo ni tolerancia
- [x] 7.8 **Corte cerrado inmutable + conciliación posterior**: un corte cerrado no cambia; un gasto resuelto tras el cierre se **liga** a su sesión de origen (no reescribe el corte ni salta al corte del día de la resolución); la verdad contable del gasto vive en su registro (D19)

## 8. Verificación

- [x] 8.1 Pruebas de aislamiento por alcance (un expendio no lee/escribe datos de otro)
- [x] 8.2 Pruebas de RBAC (permiso concede exactamente su capacidad; agregar permiso en caliente)
- [x] 8.3 Pruebas de precios (por sucursal independiente, copiar, congelado/descongelado)
- [x] 8.4 Pruebas del ciclo de solicitud (estados terminales, permiso+alcance del aprobador, **el cajero sin `autorizar_gasto` no autoriza ni propio ni ajeno**, segregación de deberes, rechazo con motivo, cancelación con motivo por un aprobador)
- [x] 8.5 Pruebas del corte: solo autorizados restan; el no autorizado aparece como faltante; cierre a ciegas; el sistema muestra la diferencia sin sanción automática; **corte secuencial** (no se abre una sesión con otra sin cerrar; día sin labor no bloquea la siguiente apertura); **corte cerrado inmutable** (autorización tardía no reescribe cifras históricas; el gasto se liga a su sesión de origen y no salta a otro corte) (D19)
- [x] 8.6 Pruebas de auditoría (una operación mutante queda atribuida a su actor; la bitácora es inmutable) y de borrado lógico (nada se borra físicamente); **marca de tiempo** en UTC del nodo (el central no la reescribe), presentada en hora local de la sucursal, y el "día" de reportes es el día local, no UTC (D21)
- [x] 8.7 Prueba de **multi-sesión concurrente** (N sesiones simultáneas sin interferencia ni degradación apreciable de latencia) (D14/D17); SLO concretos de la ruta caliente se prueban con carga en el POS
- [x] 8.8 Pruebas de **identificadores generados**: (folio) código de sucursal único e inmutable, folio secuencial por `(sucursal, tipo)` e independiente entre sucursales, sin reutilización (D18); (barcode) el generado por matriz es EAN-13 válido/decodable, **uno por producto** (idéntico en todas sus unidades), único entre productos y sin colisión con GTIN externos (D22)
- [x] 8.9 `openspec validate foundation-core --strict` en verde
