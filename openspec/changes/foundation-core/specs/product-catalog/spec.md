## ADDED Requirements

### Requirement: Producto tipado por unidad de venta, con origen aparte
El sistema SHALL clasificar cada producto por su **unidad de venta**: `peso_variable` (se vende por kilogramo, monto = precio/kg × peso real) o `pieza` (se vende por unidad, a precio fijo). El tipo SHALL ser obligatorio e inmutable tras el alta. El **origen** es un eje **independiente** del tipo: un producto puede ser **producido por matriz** o **comprado a proveedor externo**. Los productos de **peso fijo** (preempacados por matriz) se modelan como `pieza` —se cobran por unidad a precio fijo—; su peso es un dato de empaque **informativo** (opcional) que no interviene en el precio.

#### Scenario: Alta de producto peso-variable
- **WHEN** se registra "pollo" como `peso_variable`
- **THEN** el producto queda con unidad de venta kilogramo y elegible para etiquetado por peso (capacidad futura)

#### Scenario: Alta de producto pieza comprado
- **WHEN** se registra "Nescafé" como `pieza` comprado a proveedor externo
- **THEN** el producto queda con unidad de venta unidad y su código de barras es el GTIN de fábrica

#### Scenario: Alta de producto pieza producido por matriz (peso fijo)
- **WHEN** se registra "queso 500 g" como `pieza` producido por matriz
- **THEN** se cobra por unidad a precio fijo, su peso (500 g) queda como dato informativo, y el sistema le genera un código de barras (ver requisito de código de barras)

### Requirement: Código de barras del producto pieza (externo o generado por matriz)
Todo producto `pieza` SHALL tener un código de barras cuyo **origen** es `externo` (GTIN de fábrica, para productos comprados) o `matriz` (generado por el sistema, para productos producidos por matriz). El código SHALL ser **único entre productos**, sea externo o generado. Para los productos producidos por matriz, el sistema SHALL **generar** un código **EAN-13 decodable** (con dígito verificador) al darlos de alta, mediante un esquema de **prefijo interno** que no colisione con los GTIN externos ni con otros productos. El código generado es **uno por producto (SKU)**: **todas las unidades de ese producto comparten el mismo barcode**, impreso en el empaque por matriz, como un producto comercial; no es un código por-ítem. Los productos `peso_variable` SHALL NO tener código de barras de producto (se etiquetan por peso, por ítem, en una capacidad futura).

#### Scenario: GTIN externo único
- **WHEN** se registra un producto `pieza` comprado con GTIN `7501059224827`
- **THEN** ningún otro producto puede registrarse con el mismo código

#### Scenario: Matriz genera un código único y decodable
- **WHEN** matriz da de alta un producto `pieza` producido (p. ej. "salmón 500 g")
- **THEN** el sistema le genera un EAN-13 válido, único, que no colisiona con ningún GTIN externo ni con otro producto

#### Scenario: Todas las unidades comparten el mismo barcode
- **WHEN** matriz produce muchos paquetes del mismo producto (p. ej. "salmón 500 g")
- **THEN** todos llevan el **mismo** barcode impreso en el empaque, como un producto comercial —no un código distinto por unidad—

#### Scenario: Peso-variable sin código de producto
- **WHEN** se registra un producto `peso_variable`
- **THEN** el sistema no le exige ni le asigna código de barras de producto

### Requirement: Atributos base del producto
El sistema SHALL mantener por producto al menos: nombre, tipo, unidad de venta (kilogramo | unidad) y estado activo/inactivo. La unidad de venta SHALL derivarse del tipo (`peso_variable` → kilogramo, `pieza` → unidad).

#### Scenario: Unidad derivada del tipo
- **WHEN** se consulta un producto `peso_variable`
- **THEN** su unidad de venta es kilogramo sin haberse capturado manualmente

#### Scenario: Producto inactivo
- **WHEN** un producto se marca inactivo
- **THEN** no puede añadirse a nuevas ventas, conservando su historial y su información

### Requirement: Catálogo de productos global gobernado por matriz
El catálogo de productos SHALL ser **global** (único para toda la organización): la matriz da de alta los productos, que quedan disponibles para todas las sucursales. Crear o editar productos SHALL requerir el permiso de-sistema `gestionar_productos`. El **inventario** (existencias por sucursal) es una capacidad futura y NO forma parte de esta fundación; aquí solo vive el catálogo.

#### Scenario: Alta de producto en el catálogo global
- **WHEN** un usuario con `gestionar_productos` da de alta un producto
- **THEN** el producto queda disponible en el catálogo para todas las sucursales

#### Scenario: Alta de producto sin permiso
- **WHEN** un usuario sin `gestionar_productos` intenta dar de alta un producto
- **THEN** el sistema rechaza la operación
