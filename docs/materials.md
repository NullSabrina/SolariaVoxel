# Materiales de Solaria

Catalogo de diseno de rocas, tierras y maderas. Las estadisticas usan la misma
escala que el motor:

| Stat | Significado |
| --- | --- |
| **Dureza (1-10)** | Tiempo de minado. A mayor numero, mas lento. |
| **Resistencia (1-10)** | Aguante a explosiones. No depende de la dureza. |
| **Pico** | `—` = no exige pico (mano/pala/hacha). `1` madera, `2` piedra, `3` hierro, `4` diamante, `5` especial. Con un pico inferior no hay drop. |

> Estado de implementacion: el motor tiene hoy **17 bloques** (`src/world/block.rs`)
> y este catalogo es la especificacion de diseno para las rocas, tierras y
> maderas que se anadiran en siguientes versiones. Las stats de los bloques
> actuales ya viven en `src/world/mining.rs`.

## 1. Rocas igneas

| Material | Dureza | Resistencia | Pico | Mecanica especial / Drop |
| --- | :---: | :---: | :---: | --- |
| **Basalto** | 5 | 6 | 2 | Columnas hexagonales; 10% basalto pulido. |
| **Granito** | 5 | 6 | 2 | Se pule para losas; 5% cuarzo. |
| **Diorita** | 5 | 6 | 2 | Vetas mezcladas con granito; se pule. |
| **Andesita** | 4 | 5 | 1 | Roca volcanica comun; construccion temprana. |
| **Riolita** | 4 | 4 | 1 | Fragil: 25% grava ademas de riolita. |
| **Obsidiana** | 9 | 10 | 4 | Solo cae con diamante. Resiste TNT. 2% cristobalita. |
| **Piedra Pomez** | 2 | 2 | — | **Flota**: se coloca sobre agua sin hundirse. |
| **Gabro** | 6 | 7 | 2 | Oscura y pesada; 3% magnetita. |
| **Pegmatita** | 4 | 4 | 1 | 5% cuarzo, 1% berilo. |
| **Dacita** | 5 | 5 | 2 | Se agrieta en lajas decorativas. |
| **Peridotita** | 6 | 7 | 3 | 3% olivino (gema verde). |
| **Scoria** | 3 | 3 | 1 | Drena el agua adyacente. |
| **Toba volcanica** | 2 | 2 | — | Muy blanda; 10% ceniza. |
| **Obsidiana Copo de Nieve** | 9 | 9 | 5 | Pico especial; 1-3 cristobalita + bloque intacto. |
| **Kimberlita** | 6 | 6 | 3 | Matriz de diamantes: 0,5-1% diamante. |

## 2. Rocas sedimentarias

| Material | Dureza | Resistencia | Pico | Mecanica especial / Drop |
| --- | :---: | :---: | :---: | --- |
| **Arenisca** | 4 | 4 | 1 | Se talla; cae si pierde el apoyo. |
| **Caliza** | 3 | 4 | 1 | Fundir -> cal viva; fundente. |
| **Lutita** | 2 | 2 | — | Se parte en lajas: 1-2 lajas. |
| **Conglomerado** | 4 | 5 | 1 | 10% grava + 5% guijarro. |
| **Limonita** | 4 | 5 | 2 | Mineral de hierro: 10% pepita; fundir -> hierro. |
| **Travertino** | 3 | 4 | 1 | Se pule; 5% calcita. |
| **Esquisto Bituminoso** | 3 | 4 | 1 | Combustible (funde 8); fundir -> aceite. |
| **Creta** | 1 | 1 | — | Suelta 4 polvo de creta; se escribe. |
| **Silex** | 5 | 5 | 1 | Suelta 1-2 pedernal; chispa con hierro. |
| **Dolomia** | 4 | 4 | 1 | Fundente y fertilizante; neutraliza acido. |
| **Coquina** | 2 | 2 | — | Suelta conchas; fundir -> cal. |
| **Bauxita** | 3 | 4 | 1 | Fundir -> aluminio; ligera. |
| **Lignito** | 2 | 3 | — | Combustible pobre (funde 4); 10% carbon. |
| **Arcosa** | 3 | 4 | 1 | 5% feldespato (ceramica). |
| **Diatomita** | 1 | 1 | — | Absorbe agua; aislante y filtro. |
| **Caliche** | 3 | 4 | 1 | Nitrato: fertilizante y polvora. |

## 3. Rocas metamorficas

| Material | Dureza | Resistencia | Pico | Mecanica especial / Drop |
| --- | :---: | :---: | :---: | --- |
| **Marmol** | 4 | 5 | 1 | Se pule en 4 variantes; 10% calcita. |
| **Pizarra** | 3 | 3 | 1 | Lajas finas para tejados; 20% laja extra. |
| **Cuarcita** | 6 | 7 | 2 | Muy dura; 10% pedernal. |
| **Gneis** | 5 | 6 | 2 | Bandas claras/oscuras; 5% cuarzo. |
| **Esquisto** | 3 | 3 | 1 | Se parte en lajas; 15% esquirlas. |
| **Serpentinita** | 4 | 4 | 1 | Superficie resbaladiza; 5% fibra mineral. |
| **Filita** | 3 | 3 | 1 | Hojas muy finas; 10% laja. |
| **Milonita** | 4 | 4 | 2 | Roca de falla: 25% grava. |
| **Eclogita** | 6 | 7 | 3 | Solo en profundidad; 3% granate. |
| **Anfibolita** | 5 | 6 | 2 | 5% hornblenda (pigmento). |
| **Esquisto Verde** | 4 | 4 | 2 | 2% clorita (pigmento verde). |
| **Esquisto Azul** | 5 | 5 | 3 | 2% glaucofana (pigmento azul raro). |
| **Cuarcita Rosa** | 6 | 7 | 2 | Decorativa; 5% cuarzo rosa. |
| **Esteatita** | 1 | 1 | — | Resbala; suelta 4 talco. |
| **Granulita** | 6 | 7 | 3 | Alta temperatura; 3% granate. |
| **Skarn** | 5 | 6 | 2 | 5% granate + 3% magnetita. |
| **Corneana** | 6 | 7 | 2 | Se rompe en astillas: 50% esquirlas. |
| **Migmatita** | 5 | 6 | 2 | Veta clara; se pule; 5% cuarzo. |

## 4. Tierras y suelos

| Material | Dureza | Resistencia | Pico | Mecanica especial / Drop |
| --- | :---: | :---: | :---: | --- |
| **Tierra estandar** | 1 | 1 | — | Pala; con azada -> tierra de cultivo. |
| **Tierra Arcillosa** | 2 | 2 | — | Suelta 4 bolas de arcilla; retiene agua. |
| **Lodo** | 1 | 1 | — | Ralentiza 40%; al secarse -> tierra. |
| **Ceniza Volcanica** | 1 | 1 | — | Fertiliza; absorbe agua; ralentiza 15%. |
| **Grava** | 2 | 2 | — | Gravedad; 10% pedernal. |
| **Arena** | 1 | 1 | — | Gravedad; fundir -> vidrio. |
| **Arena Roja** | 1 | 1 | — | Gravedad; fundir -> terracota. |
| **Arena de Cuarzo** | 1 | 1 | — | Gravedad; 1-2 cuarzo; vidrio claro. |
| **Tierra de Cultivo** | 1 | 1 | — | Cultivos; requiere agua; se pisa. |
| **Podzol** | 1 | 1 | — | Abetos y setas; no se convierte en cultivo. |
| **Bloque de Sal** | 3 | 3 | 1 | Suelta 2-4 cristales; se disuelve en agua. |

## 5. Maderas

| Material | Dureza | Resistencia | Pico | Mecanica especial / Drop |
| --- | :---: | :---: | :---: | --- |
| **Roble** | 2 | 3 | — | Hacha; tablones y carbon vegetal. |
| **Abeto** | 2 | 3 | — | Hacha; 10% resina. |
| **Abedul** | 2 | 2 | — | Hacha; corteza -> papel. |
| **Jungla** | 2 | 3 | — | Hacha; lianas y bambu. |
| **Acacia** | 3 | 3 | — | Hacha; madera dura; tinte naranja. |
| **Palmera** | 2 | 2 | — | Hacha; 10% coco; hojas para tejados. |
| **Cerezo** | 2 | 2 | — | Hacha; petalos; madera rosa. |
| **Mangle** | 3 | 4 | — | Hacha; no se pudre en agua. |
| **Madera Petrificada** | 7 | 7 | 3 | Pico de hierro; no arde; 10% silex. |

## Notas de progresion

- **Pico**: madera (1) -> piedra (2) -> hierro (3) -> diamante (4) -> especial (5).
- **Especial (5)**: la cristobalita (obsidiana 2%, copo de nieve 1-3) + diamante
  forja el Pico Solar, que extrae intacta la Obsidiana Copo de Nieve.
- **Fundentes**: caliza, dolomia, coquina y creta.
- **Combustibles**: lignito y esquisto bituminoso.
- **Fertilizantes**: ceniza volcanica, caliche y diatomita.
- **Riesgo de bioma**: lodo y esteatita castigan el movimiento; pomez y sal
  premian la exploracion acuatica; la sal se disuelve.
