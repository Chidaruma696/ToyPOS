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
    tipo        TEXT NOT NULL CHECK (tipo IN ('Ajuste','Recepcion','Venta')),
    delta       INTEGER NOT NULL,
    motivo      TEXT,
    estado      TEXT NOT NULL CHECK (estado IN ('Aplicado','Revertido')),
    registrado  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_movimiento_prod_suc ON movimiento_inventario (producto_id, sucursal_id);

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
