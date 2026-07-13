# notifications Specification

## Purpose

Define las **notificaciones del sistema a usuarios** de ToyPOS: un mecanismo mínimo —distinto del subsistema de autorizaciones— donde el sistema (actor `sistema`) emite avisos operativos a la **bandeja de una sucursal**, de forma **no bloqueante**, auditada y sin borrado. La bandeja se consulta con el permiso `ver_notificaciones` bajo alcance, y las notificaciones se marcan **leídas a nivel sucursal** (idempotente). El tipo es un enum extensible; el primer productor es la alerta "por vencer" de la capacidad `labeling`.

## Requirements

### Requirement: Notificaciones del sistema a usuarios
El sistema SHALL poder emitir **notificaciones** dirigidas a la bandeja de una sucursal, con tipo (extensible), mensaje y marca de tiempo. La emisión SHALL ser del **sistema** (actor `sistema`), append-only y auditada en la bitácora. Las notificaciones SHALL ser **no bloqueantes**: informan, nunca detienen una operación. Este mecanismo es distinto del subsistema de autorizaciones (que resuelve solicitudes, no avisos).

#### Scenario: Emitir crea una notificación pendiente en la bandeja destino
- **WHEN** el sistema emite una notificación dirigida a una sucursal
- **THEN** la notificación aparece como no leída en la bandeja de esa sucursal y la emisión queda en la bitácora

#### Scenario: Una notificación no interrumpe la operación
- **WHEN** existe una notificación pendiente para una sucursal
- **THEN** ninguna operación de esa sucursal se bloquea por su existencia

### Requirement: Bandeja de notificaciones por sucursal dentro del alcance
El sistema SHALL exponer la **bandeja** de notificaciones de una sucursal — tanto para la administración como para el punto de venta del expendio — exigiendo el permiso `ver_notificaciones` y un alcance que cubra la sucursal. Un usuario SHALL NO poder consultar la bandeja de una sucursal fuera de su alcance.

#### Scenario: Consulta de bandeja con permiso y alcance
- **WHEN** un usuario con `ver_notificaciones` y alcance sobre la sucursal consulta su bandeja
- **THEN** el sistema lista las notificaciones de esa sucursal con su estado leída/no leída

#### Scenario: Bandeja fuera del alcance
- **WHEN** un usuario consulta la bandeja de una sucursal que su alcance no cubre
- **THEN** el sistema rechaza la operación

### Requirement: Marcar una notificación como leída
El sistema SHALL permitir marcar una notificación como **leída a nivel sucursal** (el aviso es operativo del local, no correo personal), exigiendo `ver_notificaciones` y alcance sobre la sucursal. Marcar leída SHALL registrar la marca de tiempo y SHALL NO borrar la notificación; la operación es idempotente.

#### Scenario: Marcar leída conserva la notificación
- **WHEN** un usuario con permiso y alcance marca una notificación como leída
- **THEN** la notificación queda leída con su marca de tiempo y sigue consultable en la bandeja

#### Scenario: Marcar leída dos veces no cambia nada
- **WHEN** se marca como leída una notificación ya leída
- **THEN** la operación no falla y la marca de tiempo original se conserva
