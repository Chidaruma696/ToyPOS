## ADDED Requirements

### Requirement: Registro de sucursales
El sistema SHALL permitir registrar sucursales, cada una con un tipo `matriz` o `expendio`. SHALL existir a lo sumo una sucursal de tipo `matriz`.

#### Scenario: Alta de un expendio
- **WHEN** un usuario con permiso `gestionar_sucursales` registra una sucursal de tipo `expendio`
- **THEN** la sucursal queda registrada y disponible como alcance asignable a roles

#### Scenario: Unicidad de la matriz
- **WHEN** se intenta registrar una segunda sucursal de tipo `matriz`
- **THEN** el sistema rechaza la operación

### Requirement: Cuentas de usuario
El sistema SHALL permitir crear usuarios con credenciales para autenticarse. Cada usuario SHALL poder tener uno o más roles asignados, y sus permisos efectivos SHALL ser la unión de los permisos de sus roles.

#### Scenario: Autenticación válida
- **WHEN** un usuario presenta credenciales correctas
- **THEN** el sistema establece una sesión con los permisos efectivos derivados de sus roles

#### Scenario: Credenciales inválidas
- **WHEN** un usuario presenta credenciales incorrectas
- **THEN** el sistema niega el acceso y no establece sesión

### Requirement: Catálogo de permisos atómicos
El sistema SHALL definir permisos como unidades atómicas de capacidad (por ejemplo `vender`, `editar_precio`, `registrar_gasto`, `autorizar_gasto`, `gestionar_roles`). Un permiso SHALL representar exactamente una capacidad y SHALL ser asignable de forma independiente de cualquier otro.

#### Scenario: Un permiso concede exactamente una capacidad
- **WHEN** un rol incluye el permiso `editar_precio` pero no `registrar_gasto`
- **THEN** los usuarios con ese rol pueden editar precios y no pueden registrar gastos

### Requirement: Roles componibles a medida
El sistema SHALL permitir crear y editar roles como conjuntos arbitrarios de permisos, sin un catálogo cerrado de roles predefinidos. SHALL poder agregarse o quitarse un permiso a un rol existente, y el cambio SHALL aplicar a los usuarios que ya lo tienen asignado sin recrear el rol.

#### Scenario: Agregar un permiso a un rol en caliente
- **WHEN** un admin agrega el permiso `editar_precio` al rol `etiquetador`
- **THEN** los usuarios con rol `etiquetador` obtienen la capacidad de editar precios sin recrear el rol ni reasignarlo

### Requirement: Alcance de la asignación de rol
El sistema SHALL asociar a cada asignación de rol un alcance, que es uno de: `global`, `matriz`, o un **conjunto de una o más sucursales**. El alcance SHALL determinar el conjunto de datos sobre los que los permisos del rol son efectivos.

#### Scenario: Alcance de una sola sucursal
- **WHEN** un usuario tiene el rol `cajero` con alcance `{ Expendio B }`
- **THEN** sus permisos son efectivos únicamente sobre datos de Expendio B

#### Scenario: Alcance de varias sucursales
- **WHEN** un empleado tiene el rol `cajero` con alcance `{ Expendio A, Expendio B }`
- **THEN** sus permisos son efectivos sobre Expendio A y Expendio B, y sobre ninguna otra sucursal

#### Scenario: Alcance global (administración)
- **WHEN** un usuario tiene un rol con alcance `global`
- **THEN** sus permisos pueden ser efectivos sobre todas las sucursales

### Requirement: Asignación de empleados a sucursales
El sistema SHALL exigir que cada empleado esté asignado explícitamente a su(s) sucursal(es) mediante el alcance de sus roles. Un empleado SHALL poder estar asignado a una sola sucursal, a un conjunto de sucursales, o (para administración) a todas mediante alcance `global` o `matriz`. Ningún empleado SHALL tener acceso implícito a una sucursal que no le fue asignada.

#### Scenario: Operador de una sola sucursal no se cruza
- **WHEN** se asigna a un cajero únicamente el alcance `{ Expendio B }`
- **THEN** el cajero queda asignado solo a Expendio B y no puede operar sobre ninguna otra sucursal

#### Scenario: Empleado asignado a varias sucursales
- **WHEN** un empleado trabaja en Expendio A y Expendio B
- **THEN** se le asigna alcance sobre ambas y opera en las dos sin alcanzar una tercera

### Requirement: Aislamiento de datos por alcance
El sistema SHALL filtrar toda lectura y escritura por el **alcance efectivo** del usuario (la unión de los alcances de sus asignaciones), de forma que un usuario nunca pueda leer ni modificar datos de una sucursal fuera de su alcance asignado. El aislamiento SHALL aplicarse en la capa de acceso a datos (por diseño), no por convención de la interfaz. Además, dentro de una sucursal asignada, el operador SHALL ver únicamente los datos que sus permisos autorizan, nunca más de lo autorizado.

#### Scenario: Un operador no ve datos fuera de su sucursal asignada
- **WHEN** un usuario asignado solo a Expendio B consulta ventas, notas o inventario
- **THEN** el resultado incluye solo datos de Expendio B y nunca de otra sucursal

#### Scenario: Empleado multi-sucursal ve solo sus sucursales
- **WHEN** un usuario asignado a `{ Expendio A, Expendio B }` consulta datos
- **THEN** el resultado abarca solo A y B, jamás una sucursal no asignada

#### Scenario: Dentro de la sucursal, solo lo autorizado
- **WHEN** un usuario tiene acceso a Expendio B pero sin permiso para ver cortes de caja
- **THEN** puede ver lo que sus permisos autorizan pero no los cortes de caja

#### Scenario: Alcance global ve todas las sucursales
- **WHEN** un usuario con un rol de alcance `global` consulta ventas
- **THEN** el resultado puede incluir datos de todas las sucursales

### Requirement: Protección de la gestión de identidad
El sistema SHALL requerir el permiso `gestionar_usuarios` para crear o editar usuarios, y `gestionar_roles` para crear o editar roles y su composición de permisos.

#### Scenario: Gestión de roles sin permiso
- **WHEN** un usuario sin `gestionar_roles` intenta modificar los permisos de un rol
- **THEN** el sistema rechaza la operación
