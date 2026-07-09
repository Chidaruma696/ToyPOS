## ADDED Requirements

### Requirement: Solicitud de autorización genérica
El sistema SHALL modelar las autorizaciones como **solicitudes** con: tipo (por ejemplo `gasto`, y a futuro `traspaso`), solicitante, alcance (la sucursal o ámbito al que pertenece), carga útil específica del tipo, y estado. El mecanismo SHALL ser genérico y extensible a nuevos tipos sin rediseñarlo.

#### Scenario: Crear una solicitud de gasto
- **WHEN** un cajero registra un gasto que requiere autorización
- **THEN** el sistema crea una solicitud de tipo `gasto` en estado `pendiente` con el alcance de su sucursal

### Requirement: Ciclo de estados de la solicitud
Una solicitud SHALL iniciar en `pendiente` y transicionar a `aprobada` o `rechazada`. Los estados terminales (`aprobada`, `rechazada`) SHALL NO poder modificarse. Cada resolución SHALL registrar quién resolvió y cuándo.

#### Scenario: Aprobación registra autoría
- **WHEN** un aprobador aprueba una solicitud pendiente
- **THEN** la solicitud pasa a `aprobada` y se registran el aprobador y la marca de tiempo

#### Scenario: No se re-resuelve una solicitud terminal
- **WHEN** se intenta aprobar o rechazar una solicitud ya `aprobada` o `rechazada`
- **THEN** el sistema rechaza la operación

### Requirement: El aprobador se determina por permiso y alcance
El sistema SHALL exigir que quien resuelve una solicitud tenga el permiso `autorizar_<tipo>` y un alcance que cubra el de la solicitud.

#### Scenario: Aprobador con permiso y alcance suficiente
- **WHEN** un usuario con `autorizar_gasto` y alcance `global` resuelve una solicitud de gasto de un expendio
- **THEN** el sistema le permite aprobar o rechazar

#### Scenario: Alcance insuficiente
- **WHEN** un usuario con `autorizar_gasto` pero alcance limitado a otra sucursal intenta resolver la solicitud
- **THEN** el sistema rechaza la operación

### Requirement: Efecto de la resolución delegado al tipo
El sistema SHALL aplicar el efecto de una solicitud aprobada según su tipo (por ejemplo, un gasto aprobado se vuelve un gasto legítimo). El subsistema SHALL entregar el resultado de la resolución al consumidor del tipo sin conocer su lógica interna.

#### Scenario: Gasto aprobado surte efecto
- **WHEN** una solicitud de gasto pasa a `aprobada`
- **THEN** el gasto queda marcado como autorizado para efectos del corte de caja
