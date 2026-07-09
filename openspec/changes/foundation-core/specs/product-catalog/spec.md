## ADDED Requirements

### Requirement: Producto tipado por naturaleza
El sistema SHALL clasificar cada producto como `peso_variable` (producido en matriz, se vende por kilogramo) o `pieza` (comprado a proveedor externo, se vende por unidad). El tipo SHALL ser un atributo obligatorio e inmutable tras el alta.

#### Scenario: Alta de producto peso-variable
- **WHEN** se registra "pollo" como `peso_variable`
- **THEN** el producto queda con unidad de venta kilogramo y elegible para etiquetado por peso (capacidad futura)

#### Scenario: Alta de producto pieza
- **WHEN** se registra "Nescafé" como `pieza`
- **THEN** el producto queda con unidad de venta unidad y no requiere etiquetado interno

### Requirement: Barcode de fábrica en productos pieza
El sistema SHALL almacenar el código de barras de fábrica (GTIN) de los productos `pieza`, y ese código SHALL ser único entre todos los productos. Los productos `peso_variable` SHALL NO tener código de fábrica (se etiquetan internamente en una capacidad futura).

#### Scenario: GTIN único
- **WHEN** se registra un producto `pieza` con GTIN `7501059224827`
- **THEN** ningún otro producto puede registrarse con el mismo GTIN

#### Scenario: Peso-variable sin GTIN
- **WHEN** se registra un producto `peso_variable`
- **THEN** el sistema no le exige ni le asigna código de fábrica

### Requirement: Atributos base del producto
El sistema SHALL mantener por producto al menos: nombre, tipo, unidad de venta (kilogramo | unidad) y estado activo/inactivo. La unidad de venta SHALL derivarse del tipo (`peso_variable` → kilogramo, `pieza` → unidad).

#### Scenario: Unidad derivada del tipo
- **WHEN** se consulta un producto `peso_variable`
- **THEN** su unidad de venta es kilogramo sin haberse capturado manualmente

#### Scenario: Producto inactivo
- **WHEN** un producto se marca inactivo
- **THEN** no puede añadirse a nuevas ventas, conservando su historial y su información
