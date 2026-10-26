# paint1

Editor de imágenes simple inspirado en Paint, hecho en Rust (egui + tiny-skia). A diferencia
de Paint, todo lo que se dibuja (trazos, formas, flechas, texto, imágenes) queda como un
**objeto editable** en el panel de la derecha: se puede elegir, mover, escalar, girar,
recolorear, ocultar, bloquear, renombrar o reordenar después.

## Herramientas

| Tecla | Herramienta |
|---|---|
| V | Seleccionar (mover, escalar con las manijas, girar con el círculo; Shift: proporcional / 15°) |
| P / H / E | Lápiz, resaltador, goma (la goma también es un objeto) |
| R / U / O | Rectángulo, rectángulo redondeado, elipse (Shift: cuadrado / círculo) |
| G | Polígono (clic agrega puntos; doble clic, Enter o clic en el primero termina) |
| L / A | Línea y flecha (Shift: múltiplos de 45°) |
| T | Texto (se escribe directo en el lienzo) |
| C | Recortar: libre, 1:1, 4:3, 3:2, 16:9, 3:4, 2:3, 9:16, 4:5 o tamaño exacto en px |
| I | Cuentagotas (Shift+clic: color de relleno) |

Además: rotar 90°/180°, espejo horizontal/vertical y escalar (a la selección o, si no hay, a
toda la imagen), tamaño del lienzo, **ajustar lienzo a la selección**, cortar/copiar/pegar
(los objetos copiados se pegan editables; una imagen o texto de otro programa se pega como
objeto), duplicar, deshacer/rehacer.

Atajos: Ctrl+N/O/S, Ctrl+Shift+S, Ctrl+E (exportar), Ctrl+Z / Ctrl+Y, Ctrl+X/C/V/D, Ctrl+A,
Supr, flechas (Shift: 10 px), Ctrl+0 (ajustar vista), Ctrl +/−. Rueda: zoom; botón del medio,
derecho o Espacio + arrastrar: mover la vista.

## Archivos

- **`.paint1`**: formato editable propio. Es un zip con `document.json` (lienzo y objetos) y
  las imágenes en `assets/<id>.png`.
- **Exportar**: PNG, JPEG, WebP (con pérdida o sin pérdida), BMP, GIF y TIFF, con escala y
  **tamaño máximo de archivo** opcional: primero baja la calidad (JPEG/WebP) y si no alcanza,
  la resolución.
- Abrir o arrastrar una imagen la agrega como objeto (o crea un documento de su tamaño si el
  actual está vacío).

Interfaz en español o inglés y tema claro, oscuro o del sistema (menú ☰).

## Compilar

```sh
cargo run --release -- [dibujo.paint1 | imagen.png]
cargo test
```

En Linux hace falta un compilador de C (para libwebp). `packaging/linux/paint1.desktop` y
`assets/paint1-*.png` sirven para instalarlo en el escritorio.
