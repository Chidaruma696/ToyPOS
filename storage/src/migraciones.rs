//! Esquema inicial. Convenciones **sync-ready** en toda entidad (D12): `uuid` PK
//! (TEXT), `updated_at` (TEXT, instante UTC del nodo, D21) y **borrado lógico**
//! (`activo`), sin hard delete. La `bitacora` es append-only (D11).
//!
//! Los instantes se guardan como texto RFC 3339; el dinero y el peso como
//! enteros (D6). Índices sobre las claves calientes (D17).

pub const ESQUEMA: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS sucursal (
    id          TEXT PRIMARY KEY,
    tipo        TEXT NOT NULL CHECK (tipo IN ('Matriz','Expendio')),
    codigo      TEXT NOT NULL UNIQUE,
    nombre      TEXT NOT NULL,
    zona        TEXT NOT NULL,
    activo      INTEGER NOT NULL DEFAULT 1,
    updated_at  TEXT NOT NULL
);
-- A lo sumo una matriz (índice parcial único).
CREATE UNIQUE INDEX IF NOT EXISTS ix_sucursal_matriz_unica
    ON sucursal (tipo) WHERE tipo = 'Matriz';

CREATE TABLE IF NOT EXISTS rol (
    id          TEXT PRIMARY KEY,
    nombre      TEXT NOT NULL,
    permisos    TEXT NOT NULL,              -- JSON: lista de permisos atómicos
    activo      INTEGER NOT NULL DEFAULT 1,
    updated_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS usuario (
    id            TEXT PRIMARY KEY,
    usuario       TEXT NOT NULL UNIQUE,
    hash          TEXT NOT NULL,            -- credencial argon2 (nunca texto plano)
    alcance       TEXT NOT NULL,            -- JSON: Todas | { Sucursales: [...] }
    activo        INTEGER NOT NULL DEFAULT 1,
    updated_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS usuario_rol (
    usuario_id  TEXT NOT NULL REFERENCES usuario(id),
    rol_id      TEXT NOT NULL REFERENCES rol(id),
    PRIMARY KEY (usuario_id, rol_id)
);

CREATE TABLE IF NOT EXISTS producto (
    id            TEXT PRIMARY KEY,
    nombre        TEXT NOT NULL,
    tipo          TEXT NOT NULL CHECK (tipo IN ('PesoVariable','Pieza')),
    origen        TEXT NOT NULL CHECK (origen IN ('Matriz','Externo')),
    codigo        TEXT UNIQUE,              -- EAN-13; NULL en peso-variable
    codigo_origen TEXT CHECK (codigo_origen IN ('Externo','Matriz')),
    peso_empaque  INTEGER,                  -- gramos, informativo
    vida_util     INTEGER NOT NULL DEFAULT 9, -- meses calendáricos (D33)
    activo        INTEGER NOT NULL DEFAULT 1,
    updated_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS precio (
    producto_id TEXT NOT NULL REFERENCES producto(id),
    sucursal_id TEXT NOT NULL REFERENCES sucursal(id),
    nivel       TEXT NOT NULL CHECK (nivel IN ('Menudeo','MedioMayoreo','Mayoreo')),
    monto       INTEGER NOT NULL,           -- centavos
    updated_at  TEXT NOT NULL,
    PRIMARY KEY (producto_id, sucursal_id, nivel)
);

CREATE TABLE IF NOT EXISTS sesion_caja (
    id            TEXT PRIMARY KEY,
    sucursal_id   TEXT NOT NULL REFERENCES sucursal(id),
    cajero_id     TEXT NOT NULL REFERENCES usuario(id),
    fondo         INTEGER NOT NULL,
    abierta_en    TEXT NOT NULL,
    estado        TEXT NOT NULL CHECK (estado IN ('Abierta','Cerrada')),
    contado       INTEGER,
    esperado      INTEGER,
    diferencia    INTEGER,
    gastos_autorizados INTEGER,
    ventas_efectivo    INTEGER,
    ventas_transferencia INTEGER,
    ventas_deposito      INTEGER,
    folio_corte   TEXT,
    cerrada_en    TEXT,
    updated_at    TEXT NOT NULL
);
-- A lo sumo una sesión abierta por sucursal (D13).
CREATE UNIQUE INDEX IF NOT EXISTS ix_sesion_abierta_unica
    ON sesion_caja (sucursal_id) WHERE estado = 'Abierta';

CREATE TABLE IF NOT EXISTS gasto (
    id           TEXT PRIMARY KEY,
    folio        TEXT NOT NULL UNIQUE,
    monto        INTEGER NOT NULL,
    concepto     TEXT NOT NULL,
    cajero_id    TEXT NOT NULL REFERENCES usuario(id),
    sucursal_id  TEXT NOT NULL REFERENCES sucursal(id),
    sesion_id    TEXT NOT NULL REFERENCES sesion_caja(id),
    registrado   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_gasto_sesion ON gasto (sesion_id);

CREATE TABLE IF NOT EXISTS aprobacion (
    id            TEXT PRIMARY KEY,
    gasto_id      TEXT NOT NULL UNIQUE REFERENCES gasto(id),
    sucursal_id   TEXT NOT NULL REFERENCES sucursal(id),
    solicitante_id TEXT NOT NULL REFERENCES usuario(id),
    estado        TEXT NOT NULL CHECK (estado IN ('Pendiente','Aprobada','Rechazada','Cancelada')),
    resolutor_id  TEXT,
    resuelto_en   TEXT,
    motivo        TEXT,
    updated_at    TEXT NOT NULL
);

-- Secuencia de folios por (sucursal, tipo) — generación local, sin reutilización (D18).
CREATE TABLE IF NOT EXISTS folio_seq (
    sucursal_id TEXT NOT NULL,
    tipo        TEXT NOT NULL,
    siguiente   INTEGER NOT NULL,
    PRIMARY KEY (sucursal_id, tipo)
);

-- Secuencia interna para acuñar barcodes de matriz (D22).
CREATE TABLE IF NOT EXISTS barcode_seq (
    id        INTEGER PRIMARY KEY CHECK (id = 1),
    siguiente INTEGER NOT NULL
);

-- Inventario: saldo materializado por (producto, sucursal) para lectura O(1) (D17/D23).
-- `cantidad` es la magnitud entera en la unidad del producto (unidades o gramos, D24).
-- Materialización perezosa (D30): sin fila = existencia 0.
CREATE TABLE IF NOT EXISTS existencia (
    producto_id TEXT NOT NULL REFERENCES producto(id),
    sucursal_id TEXT NOT NULL REFERENCES sucursal(id),
    cantidad    INTEGER NOT NULL,
    updated_at  TEXT NOT NULL,
    PRIMARY KEY (producto_id, sucursal_id)
);

-- Ledger de movimientos de inventario: append-only, verdad del inventario (D23).
-- `delta` lleva signo; `estado` se togglea al revertir (D27, sin apilar copias, D15).
CREATE TABLE IF NOT EXISTS movimiento_inventario (
    id          TEXT PRIMARY KEY,
    producto_id TEXT NOT NULL REFERENCES producto(id),
    sucursal_id TEXT NOT NULL REFERENCES sucursal(id),
    tipo        TEXT NOT NULL CHECK (tipo IN ('Ajuste','Envio','Recepcion','Venta')),
    delta       INTEGER NOT NULL,
    motivo      TEXT,
    estado      TEXT NOT NULL CHECK (estado IN ('Aplicado','Revertido')),
    registrado  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_movimiento_prod_suc ON movimiento_inventario (producto_id, sucursal_id);

-- Envíos entre sucursales (D38–D42): documento con folio, un extremo en la
-- matriz (D39). Su contenido vive en `envio_id` de cajas y etiquetas (D42);
-- `discrepancia` anota los faltantes de la recepción (D41).
CREATE TABLE IF NOT EXISTS envio (
    id           TEXT PRIMARY KEY,
    folio        TEXT NOT NULL UNIQUE,
    origen_id    TEXT NOT NULL REFERENCES sucursal(id),
    destino_id   TEXT NOT NULL REFERENCES sucursal(id),
    estado       TEXT NOT NULL CHECK (estado IN ('Preparado','Enviado','Recibido','Cancelado')),
    preparado_en TEXT NOT NULL,
    enviado_en   TEXT,
    recibido_en  TEXT,
    discrepancia TEXT,
    updated_at   TEXT NOT NULL
);

-- Cajas del etiquetado (D35): agrupan etiquetas por-ítem (peso_variable, cantidad
-- NULL) o unidades idénticas (pieza). `caducidad` NULL en productos externos.
CREATE TABLE IF NOT EXISTS caja_etiquetado (
    id            TEXT PRIMARY KEY,
    producto_id   TEXT NOT NULL REFERENCES producto(id),
    sucursal_id   TEXT NOT NULL REFERENCES sucursal(id),
    envio_id      TEXT REFERENCES envio(id),
    cantidad      INTEGER,
    fecha_etiquetado TEXT NOT NULL,
    caducidad     TEXT,                     -- fecha civil YYYY-MM-DD
    estado        TEXT NOT NULL CHECK (estado IN ('Activa','Extraviada','Vendida')),
    updated_at    TEXT NOT NULL
);

-- Etiquetas por-ítem (D31): identidad por etiqueta; el barcode (peso +
-- discriminador) se resuelve por lookup local. `fecha_etiquetado` es interna y
-- nunca se imprime (D32); `caducidad` es informativa, jamás bloquea.
CREATE TABLE IF NOT EXISTS etiqueta (
    id            TEXT PRIMARY KEY,
    codigo        TEXT NOT NULL UNIQUE,     -- EAN-13 per-ítem (prefijo 21)
    discriminador INTEGER NOT NULL,
    producto_id   TEXT NOT NULL REFERENCES producto(id),
    sucursal_id   TEXT NOT NULL REFERENCES sucursal(id),
    caja_id       TEXT REFERENCES caja_etiquetado(id),
    envio_id      TEXT REFERENCES envio(id), -- solo etiquetas sueltas (D42)
    peso          INTEGER NOT NULL,         -- gramos (D6)
    fecha_etiquetado TEXT NOT NULL,
    caducidad     TEXT NOT NULL,            -- fecha civil YYYY-MM-DD
    estado        TEXT NOT NULL CHECK (estado IN ('Activa','Extraviada','Vendida')),
    updated_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_etiqueta_suc_cad
    ON etiqueta (sucursal_id, caducidad) WHERE estado = 'Activa';
CREATE INDEX IF NOT EXISTS ix_etiqueta_caja ON etiqueta (caja_id);

-- Secuencia del discriminador per-ítem (módulo 10^5 con reintento, D31).
CREATE TABLE IF NOT EXISTS discriminador_seq (
    id        INTEGER PRIMARY KEY CHECK (id = 1),
    siguiente INTEGER NOT NULL
);

-- Notificaciones del sistema a usuarios (D37): bandeja por sucursal, se marcan
-- leídas por sucursal y nunca se borran. `grupo` es la clave de idempotencia
-- del barrido "por vencer" (D36).
CREATE TABLE IF NOT EXISTS notificacion (
    id          TEXT PRIMARY KEY,
    tipo        TEXT NOT NULL CHECK (tipo IN ('PorVencer','DiscrepanciaEnvio')),
    mensaje     TEXT NOT NULL,
    sucursal_id TEXT NOT NULL REFERENCES sucursal(id),
    grupo       TEXT,
    creado      TEXT NOT NULL,
    leida_en    TEXT,
    updated_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_notificacion_bandeja ON notificacion (sucursal_id, creado);
CREATE INDEX IF NOT EXISTS ix_notificacion_grupo ON notificacion (grupo);

-- Bitácora append-only e inmutable (D11).
CREATE TABLE IF NOT EXISTS bitacora (
    id         TEXT PRIMARY KEY,
    ts         TEXT NOT NULL,
    actor      TEXT NOT NULL,               -- 'usuario:<uuid>' | 'sistema'
    operacion  TEXT NOT NULL CHECK (operacion IN ('Crear','Modificar','Eliminar')),
    entidad    TEXT NOT NULL,
    entidad_id TEXT NOT NULL,
    alcance    TEXT NOT NULL,
    antes      TEXT,
    despues    TEXT
);
CREATE INDEX IF NOT EXISTS ix_bitacora_entidad ON bitacora (entidad, entidad_id);
"#;

/// Columnas añadidas después del esquema inicial. **Idempotentes** por manejo
/// del error: si la columna ya existe (base nueva, ya viene en el `CREATE
/// TABLE`), el "duplicate column" se ignora al aplicar el esquema.
///
/// **Nota sobre los `CHECK` de enums** (estado/tipo): SQLite no permite
/// modificarlos con `ALTER`; el esquema canónico de arriba los lleva al día y
/// el guardián real son los enums de Rust. En pre-release no hay despliegues
/// con datos: una base vieja cuyo `CHECK` rechace un valor nuevo falla en voz
/// alta y se recrea (las bases de prueba son efímeras).
pub const COLUMNAS_ADITIVAS: [&str; 3] = [
    "ALTER TABLE producto ADD COLUMN vida_util INTEGER NOT NULL DEFAULT 9",
    "ALTER TABLE caja_etiquetado ADD COLUMN envio_id TEXT REFERENCES envio(id)",
    "ALTER TABLE etiqueta ADD COLUMN envio_id TEXT REFERENCES envio(id)",
];

/// Disparadores que hacen la bitácora **inmutable** desde SQL (belt-and-suspenders
/// sobre la inmutabilidad de la aplicación). Se ejecutan uno a uno porque llevan
/// `;` internos en el bloque `BEGIN … END`.
pub const TRIGGERS: [&str; 2] = [
    "CREATE TRIGGER IF NOT EXISTS bitacora_no_update \
     BEFORE UPDATE ON bitacora \
     BEGIN SELECT RAISE(ABORT, 'la bitácora es inmutable'); END",
    "CREATE TRIGGER IF NOT EXISTS bitacora_no_delete \
     BEFORE DELETE ON bitacora \
     BEGIN SELECT RAISE(ABORT, 'la bitácora es inmutable'); END",
];
