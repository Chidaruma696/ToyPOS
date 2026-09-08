## ADDED Requirements

### Requirement: Registro de sucursales
El sistema SHALL permitir registrar sucursales, cada una con un tipo `matriz` o `expendio`, un **código** corto alfanumérico y una **zona horaria** (identificador IANA, p. ej. `America/Mexico_City`, valor por defecto). SHALL existir a lo sumo una sucursal de tipo `matriz`. El **código** SHALL ser **único** entre todas las sucursales e **inmutable** tras el alta, SHALL asignarlo quien crea la sucursal (no se deriva del nombre) y SHALL prefijar los folios de los documentos de esa sucursal (ver capacidad `cash-management` y D18). La **zona horaria** define la hora local de la sucursal para presentación y para el "día" de sus reportes/cortes (ver requisito de marca de tiempo y D21). Renombrar una sucursal SHALL NO cambiar su código ni los folios históricos.

#### Scenario: Alta de un expendio
- **WHEN** un usuario con permiso `gestionar_sucursales` registra una sucursal de tipo `expendio` con código `SRO`
- **THEN** la sucursal queda registrada con código `SRO` y disponible como alcance asignable a roles

#### Scenario: Unicidad de la matriz
- **WHEN** se intenta registrar una segunda sucursal de tipo `matriz`
- **THEN** el sistema rechaza la operación

#### Scenario: Códigos distintos para nombres parecidos
- **WHEN** se registran Santa Rosa con código `SRO` y Santa Rita con código `SRI`
- **THEN** ambas quedan con códigos únicos y sus folios nunca colisionan

#### Scenario: Código duplicado rechazado
- **WHEN** se intenta registrar una sucursal con un código que ya usa otra
- **THEN** el sistema rechaza la operación

#### Scenario: Renombrar no altera el código
- **WHEN** se cambia el nombre de una sucursal con código `VAL`
- **THEN** el código sigue siendo `VAL` y los folios ya emitidos no se alteran

#### Scenario: Zona horaria por defecto
- **WHEN** se registra una sucursal sin especificar zona horaria
- **THEN** queda con `America/Mexico_City` como zona local, aplicable a la presentación y al "día" de sus reportes

### Requirement: Cuentas de usuario
El sistema SHALL permitir crear usuarios que se autentican con **usuario y contraseña**; la contraseña SHALL almacenarse **hasheada** (nunca en texto plano) y validarse **localmente** contra el nodo, de modo que el login funcione sin red. Cada usuario SHALL poder tener uno o más roles asignados, y sus permisos efectivos SHALL ser la unión de los permisos de sus roles. Los permisos son una **bolsa plana independiente del alcance**: determinan el *qué*, no el *dónde*.

#### Scenario: Autenticación válida (incluso sin conexión)
- **WHEN** un usuario presenta credenciales correctas, con o sin red
- **THEN** el sistema valida localmente y establece una sesión con su `ContextoAcceso` (permisos efectivos + alcance)

#### Scenario: Credenciales inválidas
- **WHEN** un usuario presenta credenciales incorrectas
- **THEN** el sistema niega el acceso y no establece sesión

### Requirement: Cierre de sesión por inactividad
El sistema SHALL cerrar por completo la sesión tras un periodo de inactividad; para volver, el usuario SHALL hacer un **login completo** (no existe desbloqueo por PIN). Hay **una sola forma de entrar**.

#### Scenario: La sesión inactiva se cierra
- **WHEN** una sesión permanece inactiva más allá del periodo configurado
- **THEN** el sistema la cierra y exige un login completo para volver a operar

### Requirement: Sucursal activa dentro del alcance
El sistema SHALL determinar la sucursal activa **dentro del alcance** del usuario: si su alcance es una sola sucursal, esa es la activa sin pedir selección; si abarca varias o todas (matriz), el usuario elige/filtra la activa dentro de la aplicación (por defecto, matriz). La selección de sucursal activa SHALL NO ampliar el alcance: solo filtra dentro de lo que el alcance ya permite.

#### Scenario: Usuario de una sola sucursal entra directo
- **WHEN** un usuario con alcance `{ Expendio B }` inicia sesión
- **THEN** opera directamente sobre Expendio B sin elegir sucursal

#### Scenario: Usuario multi-sucursal elige la activa
- **WHEN** un usuario con alcance sobre varias sucursales inicia sesión
- **THEN** elige dentro de la app sobre cuál operar, sin poder salir de su alcance

### Requirement: Catálogo de permisos atómicos
El sistema SHALL definir permisos como unidades atómicas de capacidad (por ejemplo `vender`, `editar_precio`, `registrar_gasto`, `autorizar_gasto`, `gestionar_roles`). Un permiso SHALL representar exactamente una capacidad (**nivel capacidad**, no un permiso por elemento visual) y SHALL ser asignable de forma independiente de cualquier otro. Cada permiso SHALL clasificarse como **por-sucursal** —se evalúa como `tiene(permiso) Y el alcance cubre la sucursal objetivo`— o **de-sistema** —acción global sin sucursal objetivo, basta `tiene(permiso)`.

#### Scenario: Un permiso concede exactamente una capacidad
- **WHEN** un rol incluye el permiso `editar_precio` pero no `registrar_gasto`
- **THEN** los usuarios con ese rol pueden editar precios y no pueden registrar gastos

#### Scenario: Permiso por-sucursal se evalúa contra la sucursal
- **WHEN** un usuario con `vender` y alcance `{ Expendio A }` intenta vender en Expendio B
- **THEN** el sistema rechaza la operación porque su alcance no cubre B

#### Scenario: Permiso de-sistema no depende de sucursal
- **WHEN** un usuario con `gestionar_roles` edita un rol
- **THEN** el sistema lo permite sin evaluar ninguna sucursal

### Requirement: Roles componibles a medida
El sistema SHALL permitir crear y editar roles como conjuntos arbitrarios de permisos, sin un catálogo cerrado de roles predefinidos. SHALL poder agregarse o quitarse un permiso a un rol existente, y el cambio SHALL aplicar a los usuarios que ya lo tienen asignado sin recrear el rol.

#### Scenario: Agregar un permiso a un rol en caliente
- **WHEN** un admin agrega el permiso `editar_precio` al rol `etiquetador`
- **THEN** los usuarios con rol `etiquetador` obtienen la capacidad de editar precios sin recrear el rol ni reasignarlo

### Requirement: Alcance del usuario
El sistema SHALL asociar a cada **usuario** un alcance: el conjunto de sucursales sobre las que opera, que es una sola sucursal, varias, o **todas** (nivel matriz/administración). El alcance es **del usuario** (no de sus roles) y es **independiente de sus permisos**: determina el *dónde*, no el *qué*.

#### Scenario: Alcance de una sola sucursal
- **WHEN** un usuario tiene alcance `{ Expendio B }`
- **THEN** sus permisos son efectivos únicamente sobre datos de Expendio B

#### Scenario: Alcance de varias sucursales
- **WHEN** un usuario tiene alcance `{ Expendio A, Expendio B }`
- **THEN** opera sobre Expendio A y Expendio B, y sobre ninguna otra sucursal

#### Scenario: Alcance de todas (matriz) no concede permisos por sí mismo
- **WHEN** un usuario tiene alcance = todas las sucursales pero no tiene el permiso `autorizar_gasto`
- **THEN** puede ver datos de todas las sucursales pero no puede autorizar gastos en ninguna

### Requirement: Asignación de empleados a sucursales
El sistema SHALL exigir que cada empleado esté asignado explícitamente a su(s) sucursal(es) mediante **su alcance**. Un empleado SHALL poder estar asignado a una sola sucursal, a un conjunto de sucursales, o (para administración) a **todas** (nivel matriz). Ningún empleado SHALL tener acceso implícito a una sucursal que no le fue asignada.

#### Scenario: Operador de una sola sucursal no se cruza
- **WHEN** se asigna a un cajero únicamente el alcance `{ Expendio B }`
- **THEN** el cajero queda asignado solo a Expendio B y no puede operar sobre ninguna otra sucursal

#### Scenario: Empleado asignado a varias sucursales
- **WHEN** un empleado trabaja en Expendio A y Expendio B
- **THEN** se le asigna alcance sobre ambas y opera en las dos sin alcanzar una tercera

### Requirement: Aislamiento de datos por alcance
El sistema SHALL filtrar toda lectura y escritura por el **alcance** del usuario (el conjunto de sucursales que tiene asignado), de forma que un usuario nunca pueda leer ni modificar datos de una sucursal fuera de su alcance asignado. El aislamiento SHALL aplicarse en la capa de acceso a datos (por diseño), no por convención de la interfaz. Además, dentro de una sucursal asignada, el operador SHALL ver únicamente los datos que sus permisos autorizan, nunca más de lo autorizado.

#### Scenario: Un operador no ve datos fuera de su sucursal asignada
- **WHEN** un usuario asignado solo a Expendio B consulta ventas, notas o inventario
- **THEN** el resultado incluye solo datos de Expendio B y nunca de otra sucursal

#### Scenario: Empleado multi-sucursal ve solo sus sucursales
- **WHEN** un usuario asignado a `{ Expendio A, Expendio B }` consulta datos
- **THEN** el resultado abarca solo A y B, jamás una sucursal no asignada

#### Scenario: Dentro de la sucursal, solo lo autorizado
- **WHEN** un usuario tiene acceso a Expendio B pero sin permiso para ver cortes de caja
- **THEN** puede ver lo que sus permisos autorizan pero no los cortes de caja

#### Scenario: Alcance de todas las sucursales ve todo
- **WHEN** un usuario con alcance = todas las sucursales consulta ventas
- **THEN** el resultado puede incluir datos de todas las sucursales

### Requirement: Protección de la gestión de identidad
El sistema SHALL requerir el permiso `gestionar_usuarios` para crear o editar usuarios, y `gestionar_roles` para crear o editar roles y su composición de permisos.

#### Scenario: Gestión de roles sin permiso
- **WHEN** un usuario sin `gestionar_roles` intenta modificar los permisos de un rol
- **THEN** el sistema rechaza la operación

### Requirement: Bitácora de auditoría de operaciones
El sistema SHALL registrar en una bitácora **append-only e inmutable** toda operación que **cree, modifique o elimine** datos, con al menos: actor, tipo de operación, entidad afectada, valores antes y después (para modificaciones), alcance y marca de tiempo. El **actor** SHALL ser un **usuario** o el **`sistema`**; las operaciones ejecutadas sin usuario en sesión (el arranque inicial, y las operaciones automáticas futuras) SHALL registrarse con actor `sistema`, de modo que ninguna escritura quede sin atribuir y el registro distinga lo automático de lo humano. La bitácora SHALL capturarse en la capa de acceso a datos —el mismo punto único que aplica el aislamiento por alcance— de modo que ninguna operación mutante, de cualquier capacidad presente o futura, pueda ejecutarse sin quedar registrada. La bitácora SHALL NO poder editarse ni borrarse desde la aplicación.

#### Scenario: Un cambio de precio queda atribuido
- **WHEN** un usuario cambia el precio de menudeo de "huevo" de 40.00 a 35.00
- **THEN** la bitácora registra el actor, la entidad, el cambio 40.00 → 35.00 y la marca de tiempo

#### Scenario: Una modificación deliberada es rastreable
- **WHEN** un empleado ajusta un dato del sistema
- **THEN** la bitácora identifica quién lo hizo y qué cambió, de modo que la acción es atribuible

#### Scenario: Una operación sin usuario queda atribuida al sistema
- **WHEN** el arranque inicial crea la matriz y el admin (o una operación automática futura modifica datos) sin un usuario en sesión
- **THEN** la bitácora registra el actor `sistema`, de modo que la operación sigue siendo atribuible

#### Scenario: La bitácora no se puede alterar
- **WHEN** se intenta editar o borrar una entrada de la bitácora desde la aplicación
- **THEN** el sistema rechaza la operación

### Requirement: Marca de tiempo en UTC, presentada en hora local
Toda marca de tiempo (bitácora, documentos, `updated_at`) SHALL guardarse como **instante en UTC**, capturado en el **nodo** donde ocurre la operación. Ningún componente de almacenamiento ni servidor central SHALL reescribir o sustituir esa marca. Para presentación (bitácora, tickets, dashboard) y para definir el **día** de reportes y cortes, el instante SHALL convertirse a la **zona horaria de la sucursal** correspondiente (identificador IANA), **nunca** mediante un desfase fijo hardcodeado.

#### Scenario: La bitácora muestra la hora local real
- **WHEN** una operación ocurre a las 10:00 en la hora local de una sucursal en `America/Mexico_City`
- **THEN** se guarda el instante UTC correspondiente y la bitácora lo muestra como las 10:00 en la hora local de esa sucursal, no la hora de un servidor remoto

#### Scenario: El servidor central no reescribe la hora
- **WHEN** un registro creado localmente se sincroniza a un servidor central ubicado en otra zona
- **THEN** conserva su instante UTC original, sin que el central lo sustituya por su propia hora

#### Scenario: El día de reportes es el día local de la sucursal
- **WHEN** se agregan cortes o movimientos "por día" para un reporte o el dashboard
- **THEN** el corte del "día" se determina por el día natural en la zona horaria de la sucursal, no por el día UTC

### Requirement: Bootstrap del sistema inicial
En un sistema nuevo (sin datos), SHALL existir un mecanismo de arranque que cree la **sucursal matriz** y un **usuario administrador inicial** con alcance de todas las sucursales y los permisos de-sistema necesarios para gestionar usuarios, roles, sucursales y productos. El mecanismo concreto (seed, asistente de primer arranque) se define en implementación.

#### Scenario: Arranque de un sistema vacío
- **WHEN** el sistema se inicializa por primera vez, sin usuarios ni sucursales
- **THEN** quedan creadas la sucursal matriz y un usuario administrador capaz de gestionar la organización
