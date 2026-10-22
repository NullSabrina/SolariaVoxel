// tools/gen_gui.js — regenera assets/gui.png (256x256) para Solaria Voxel.
//
// Ejecutar en LibreSprite (via el servidor MCP `libresprite_run_script`, o
// abriendolo desde el menu Scripts). Requiere que `assets/gui.png` exista (la
// primera vez, crea un PNG 256x256 en blanco). Es **idempotente**: rellenar las
// regiones dos veces da el mismo resultado.
//
// Dibuja el arte de interfaz con el **bisel** de Minecraft (luz arriba-izquierda,
// sombra abajo-derecha) y guarda el PNG + el `.aseprite` fuente. El manifiesto de
// regiones (coordenadas) vive en `assets/gui.json`.

var CANVAS = "assets/gui.png";
var SOURCE = "assets/src/ui/gui.aseprite";

app.open(CANVAS);
var img = app.activeImage;
var col = app.pixelColor;

function put(x, y, r, g, b, a) { img.putPixel(x, y, col.rgba(r, g, b, a)); }
function fill(x0, y0, w, h, c) {
  for (var y = 0; y < h; ++y) for (var x = 0; x < w; ++x) put(x0 + x, y0 + y, c[0], c[1], c[2], c[3]);
}
// Bisel: base + contorno exterior (1px) + bisel interior (luz arriba-izq, sombra abajo-der).
function bevel(x0, y0, w, h, base, light, dark, border) {
  fill(x0, y0, w, h, base);
  if (border) {
    for (var x = 0; x < w; ++x) { put(x0 + x, y0, border[0], border[1], border[2], border[3]); put(x0 + x, y0 + h - 1, border[0], border[1], border[2], border[3]); }
    for (var y = 0; y < h; ++y) { put(x0, y0 + y, border[0], border[1], border[2], border[3]); put(x0 + w - 1, y0 + y, border[0], border[1], border[2], border[3]); }
  }
  if (w >= 3 && h >= 3) {
    for (var x = 1; x < w - 1; ++x) { put(x0 + x, y0 + 1, light[0], light[1], light[2], light[3]); put(x0 + x, y0 + h - 2, dark[0], dark[1], dark[2], dark[3]); }
    for (var y = 1; y < h - 1; ++y) { put(x0 + 1, y0 + y, light[0], light[1], light[2], light[3]); put(x0 + w - 2, y0 + y, dark[0], dark[1], dark[2], dark[3]); }
  }
}

var DARK = [13, 6, 0, 255], INNER = [36, 18, 9, 255], WOOD = [78, 53, 30, 255], BL = [96, 68, 40, 255];
// Hotbar (0,0) 182x22 + 9 ranuras.
bevel(0, 0, 182, 22, WOOD, BL, DARK, null);
for (var i = 0; i < 9; ++i) bevel(1 + i * 20, 1, 20, 20, INNER, DARK, WOOD, DARK);
// Ranuras sueltas: normal, hover, seleccionada.
bevel(0, 24, 20, 20, INNER, DARK, WOOD, DARK);
bevel(20, 24, 20, 20, [58, 36, 18, 255], DARK, WOOD, DARK);
bevel(40, 24, 20, 20, [158, 126, 68, 255], [214, 186, 112, 255], [96, 74, 36, 255], [30, 22, 10, 255]);
// Panel (0,48) 200x76.
bevel(0, 48, 200, 76, [28, 19, 12, 255], [58, 41, 28, 255], [9, 6, 3, 255], [7, 4, 2, 255]);
// Botones 200x20: normal, hover, pulsado, desactivado.
bevel(0, 128, 200, 20, [56, 41, 29, 255], [102, 77, 50, 255], [26, 18, 12, 255], [12, 8, 4, 255]);
bevel(0, 150, 200, 20, [78, 57, 38, 255], [134, 101, 65, 255], [38, 26, 17, 255], [14, 9, 5, 255]);
bevel(0, 172, 200, 20, [36, 26, 18, 255], [24, 17, 11, 255], [92, 68, 44, 255], [10, 7, 3, 255]);
bevel(0, 194, 200, 20, [40, 36, 33, 255], [62, 57, 52, 255], [24, 21, 19, 255], [12, 10, 8, 255]);
// Pestana (204,112) 40x14.
bevel(204, 112, 40, 14, [110, 110, 110, 255], [168, 168, 168, 255], [52, 52, 52, 255], [30, 30, 30, 255]);
// Flecha de crafteo (204,64) 32x16.
for (var y = 0; y < 16; ++y) for (var x = 0; x < 32; ++x) {
  var inB = y >= 6 && y <= 9 && x < 22;
  var inH = x >= 20 && Math.abs(y - 8) <= Math.floor((31 - x) / 2) + 1;
  if (!(inB || inH)) continue;
  var e = y === 6 || y === 9 || x === 0;
  put(204 + x, 64 + y, e ? 138 : 216, e ? 115 : 196, e ? 85 : 154, 255);
}
// Dim (244,64) 8x8.
fill(244, 64, 8, 8, [0, 0, 0, 170]);
// Mirilla (204,90) 15x15.
for (var i = 3; i <= 11; ++i) { put(211, 90 + i, 0, 0, 0, 255); put(204 + i, 97, 0, 0, 0, 255); }
for (var i = 4; i <= 10; ++i) { put(211, 90 + i, 255, 255, 255, 255); put(204 + i, 97, 255, 255, 255, 255); }

app.activeSprite.saveAs(CANVAS, true);
app.activeSprite.saveAs(SOURCE, true);
console.log("OK: " + CANVAS + " + " + SOURCE);
