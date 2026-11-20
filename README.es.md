<p align="center">
  <img src="assets/santacita-icon.svg" width="144" height="144" alt="SantaCita logo">
</p>

<h1 align="center">SantaCita</h1>

<p align="center">
  Asistente para citas de <b>Toma de huellas / TIE</b> y <b>Autorización de regreso</b> en ICP Plus.<br>
  <sub>Gratis · Código disponible · macOS, Windows, Linux</sub>
</p>

<p align="center">

🇪🇸 **Español** · 🇬🇧 [English](README.md) · 🇷🇺 [Русский](README.ru.md)

</p>

<p align="center">

[![Última versión publicada](https://img.shields.io/github/v/release/itwormz/santacita?sort=semver&label=release&color=626bdf&logo=github)](https://github.com/itwormz/santacita/releases)

</p>

Nueva aplicación en Rust, Tauri y Svelte.

SantaCita 0.4 y posteriores es una aplicación ligera y rápida escrita en Rust que funciona con el navegador que ya tiene instalado. Como no incluye ninguno, con algunas configuraciones pueden surgir problemas. En ese caso, pruebe la versión anterior, la [0.3.8](https://github.com/itwormz/santacita/releases/tag/v0.3.8), que incluye su propio navegador Camoufox.

Las versiones anteriores a la 0.4.0 son una aplicación en Python; su código permanece en la rama [`ceh-python`](https://github.com/itwormz/santacita/tree/ceh-python).

## 🚀 Inicio rápido

1. Abra [Releases](https://github.com/itwormz/santacita/releases) y descargue la última versión de SantaCita para su sistema.
2. Instale la aplicación y seleccione un navegador instalado: Chrome, Brave, Edge, Chromium o Firefox. El navegador no se incluye; los perfiles nuevos empiezan con el primer navegador basado en Chromium que se encuentre (primero Chrome) y con Firefox si no hay ninguno.
3. Cree un perfil con provincia, trámite, autenticación y datos del solicitante. Inicie la búsqueda y autentíquese en el navegador.
4. Cuando reciba un aviso, elija oficina o termine la reserva. **Fecha, hora, CAPTCHA y confirmación final siguen siendo manuales.**

SantaCita abre un perfil separado del navegador seleccionado; no utiliza su perfil habitual ni sus pestañas. Edge solo es compatible en Windows. En Linux no se admiten las versiones snap de Firefox y Chromium; utilice una instalación que no sea snap.

SantaCita es compatible con macOS (Apple Silicon e Intel), Windows 10 y 11 (x64) y Linux (x64).

| Sistema | Archivo |
| --- | --- |
| macOS Apple Silicon | `SantaCita-<versión>-macOS-Apple-Silicon.dmg` |
| macOS Intel | `SantaCita-<versión>-macOS-Intel.dmg` |
| Windows x64 | `SantaCita-<versión>-windows-x64.msi` o `-portable.zip` |
| Linux x64 | `SantaCita-<versión>-x86_64.AppImage` |

macOS: arrastre SantaCita a Applications. No está notarizada; si se bloquea, use Ajustes del Sistema → Privacidad y seguridad → Abrir igualmente. Los paquetes no están firmados con un certificado de desarrollador de pago; compárelos con `SHA256SUMS` y el paquete Sigstore que se publica con cada versión. Windows: los paquetes no tienen firma de editor; SmartScreen puede requerir Más información → Ejecutar de todas formas. Linux: dé permiso de ejecución al AppImage (`chmod +x archivo.AppImage`). Un AppImage necesita FUSE 3 (`fusermount3`, paquete `fuse3`): suele venir preinstalado en los escritorios; si no, instálelo (`sudo apt install fuse3`, `sudo dnf install fuse3`, `sudo pacman -S fuse3`) o inicie el archivo con `--appimage-extract-and-run`. El Firefox propio de Ubuntu es un snap y no es compatible: instale Chrome desde su paquete `.deb` o Firefox desde el repositorio APT de Mozilla.

## 🔄 Búsqueda y controles

- Espera y actualiza según los ajustes mientras el formulario indica que no hay citas.
- Cuando desaparece el mensaje, empieza un nuevo recorrido desde la autenticación y vuelve a comprobar.
- Selecciona automáticamente una única oficina disponible; si hay varias, pide su elección.
- Pausa, reanudación y continuación tras elegir fecha desde la aplicación. Los tiempos y la protección contra bloqueos se aplican durante la búsqueda; los demás cambios del perfil, en el siguiente inicio.
- Las estadísticas y el registro se conservan entre búsquedas. Puede añadir una reserva confirmada al calendario y compartir sus estadísticas.

## 🔔 Notificaciones

Los perfiles admiten sonido, Telegram y webhook. Telegram se configura con token de bot, destinatarios y eventos: acceso, elección de oficina, elección de cita, búsqueda detenida (también una página que el bot no reconoce). El webhook requiere dirección HTTPS y eventos; cabeceras y plantilla son opcionales.

Los secretos se guardan en el almacén de credenciales del sistema. macOS puede pedir la contraseña del Llavero para acceder al token de Telegram.

## 🗂️ Importar datos

Al iniciarse por primera vez, SantaCita busca la aplicación Python instalada y copia perfiles, estadísticas, logros y registros. Los originales se conservan; SantaCita usa una carpeta de datos `SantaCita` independiente. La importación desde Ajustes añade datos que faltan sin sustituir sus perfiles actuales.

Cierre Python antes de importar. Debe autenticarse de nuevo en el navegador. La carpeta portable antigua no se detecta automáticamente: conserve su carpeta `data` y use una copia separada para migrar.

**Windows portable:** extraiga el ZIP completo. `portable.flag` guarda los datos en la carpeta `data` junto al programa. Para migrar datos portable anteriores, copie la antigua carpeta `data` a la nueva carpeta portable antes del primer inicio y conserve el original.

## ⬆️ Actualizaciones

Comprobación en Acerca de, o automáticamente al iniciar y una vez al día. **Ofrecer versiones beta** incluye versiones estables y las betas de versiones posteriores (0.5.0 y superiores); está desactivado por defecto y se importa desde Python 0.3.8.

La descarga e instalación requieren su acción. Se verifican SHA256 y la procedencia Sigstore de la compilación, también antes de instalar. Un fallo de verificación bloquea la instalación. Windows portable sustituye el programa y se reinicia conservando `data`.

## ⚠️ Limitaciones

- Requiere un navegador compatible instalado y acceso al almacén de credenciales para las notificaciones configuradas.
- Windows usa WebView2. El instalador MSI lo instala solo si falta (hace falta acceso a la red; volver a ejecutar el MSI repara un intento fallido). Si no funciona, y para el ZIP portable, instale [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/).
- Requisitos de Linux: escritorio gráfico, glibc 2.35 o superior (Ubuntu 22.04, Debian 12, Fedora 36 y posteriores), `fuse3` para el AppImage, las bibliotecas Mesa GL/EGL, un llavero Secret Service (gnome-keyring o KWallet) para las credenciales de notificaciones y un navegador que no sea snap. Probado en contenedores con Ubuntu 22.04, Fedora 37, Kali y Arch; la GPU real, Wayland, la bandeja y las notificaciones aún no están verificados. En Wayland nativo puede ser necesario abrir la ventana del navegador manualmente.
- Pueden producirse errores del sitio, límites HTTP 429 y nuevas autenticaciones. No se garantiza obtener cita.
- [Diagnóstico y privacidad](docs/PRIVACY.md). Elimine datos personales antes de compartir informes.

## 🎯 Finalidad

Proyecto independiente para obtener una cita propia, sin vinculación con las autoridades españolas. No está destinado a revender citas ni a acapararlas masivamente.

Se ofrece tal cual, sin garantía de funcionamiento ni de obtener cita. El usuario es responsable de cumplir la legislación y las reglas del servicio.

## 📣 Un mensaje a las autoridades responsables

El acceso a un trámite administrativo obligatorio no debería depender de la capacidad de una persona para actualizar una página durante semanas, adivinar cuándo aparecen citas o recurrir a intermediarios de pago.

A mi juicio, un sistema que exige este comportamiento a las personas extranjeras menoscaba su dignidad y crea una barrera efectiva de acceso a los servicios públicos. Son necesarios una cola transparente, plazos de espera comprensibles, avisos de disponibilidad y una alternativa accesible a la cita por internet.

Las medidas contra la automatización también pueden dificultar el acceso de los usuarios que utilizan el servicio manualmente. La protección del servicio debe contemplar los falsos positivos y ofrecer un procedimiento claro para recuperar el acceso. Una persona que intenta cumplir un trámite administrativo obligatorio no debería tener que comprender los mecanismos técnicos de protección contra bots.

Este proyecto existe porque el procedimiento habitual para obtener una cita exige un esfuerzo desproporcionado. Espero que las mejoras del servicio oficial hagan innecesarios asistentes como este.

### El mercado paralelo y su coste público

Un procedimiento de cita poco transparente y las dificultades de acceso por los canales habituales crean condiciones favorables para un mercado paralelo en el que el acceso a un servicio público gratuito se convierte en mercancía. El Ministerio del Interior ha informado de investigaciones sobre el acaparamiento masivo de citas mediante automatización para su posterior reventa. Estas prácticas dificultan aún más el acceso de quienes intentan conseguir su propia cita.

Las consecuencias van más allá de las dificultades para las personas extranjeras: combatir el acaparamiento y la reventa requiere investigaciones policiales, análisis técnicos especializados e intervención judicial. A mi juicio, un sistema de asignación accesible y transparente debe abordar las condiciones que favorecen este mercado, junto con la persecución de los abusos.

Las citas de un servicio público deben distribuirse conforme a reglas claras y justas, sin convertirse en una competición de capacidades técnicas o de poder adquisitivo.

Ejemplos oficiales de investigaciones:

- **12 de mayo de 2023:** el Ministerio informó de 69 detenidos y otras 25 personas investigadas como presuntas responsables de pertenencia a organización criminal y daños informáticos. La investigación estaba judicializada en el Juzgado de Instrucción n.º 8 de Valencia; según la Policía, las citas se acaparaban para revenderlas por 30–200 euros. [Comunicado del Ministerio](https://www.interior.gob.es/opencms/es/detalle/articulo/La-Policia-Nacional-desmantela-un-entramado-que-bloqueaba-el-sistema-de-citas-online-de-extranjeria-mediante-un-bot-informatico/).
- **27 de noviembre de 2024:** el Ministerio informó de 21 detenidos en Alicante, Murcia y Valencia en una investigación que incluía venta de citas, bloqueo del sistema mediante bots y falsificación de documentos. [Comunicado del Ministerio](https://www.interior.gob.es/opencms/es/detalle/articulo/La-Policia-Nacional-desarticula-un-grupo-criminal-dedicado-a-la-venta-fraudulenta-de-citas-de-extranjeria-mediante-bots/).

Estos comunicados describen investigaciones y detenciones, no acreditan la existencia de sentencias condenatorias firmes.

### Sus derechos y dónde acudir

- **Base jurídica:** los artículos 12 y 13 de la [Ley 39/2015](https://www.boe.es/eli/es/l/2015/10/01/39) contemplan la asistencia en el uso de medios electrónicos y el derecho a un trato respetuoso que facilite el ejercicio de derechos y el cumplimiento de obligaciones. Estas disposiciones no autorizan la automatización.
- **Errores técnicos y bloqueos:** la [ayuda oficial de cita previa](https://sede.administracionespublicas.gob.es/pagina/index/directorio/ayuda_cita_previa) incluye el teléfono 060 y un formulario de consultas técnicas. Este canal atiende incidencias técnicas.
- **Quejas sobre el acceso a servicios públicos:** acudir al Defensor del Pueblo es gratuito, está al alcance de las personas extranjeras con independencia de su situación legal y no requiere abogado. [Quién puede quejarse y cómo](https://www.defensordelpueblo.es/el-defensor/como-te-podemos-ayudar/) · [Presentar una queja](https://portal-sac.defensordelpueblo.es/es/form/queja). La presentación de una queja no suspende los plazos administrativos.
- **Recomendaciones oficiales:** el Defensor del Pueblo ya ha [reclamado mejoras en el acceso a las citas](https://www.defensordelpueblo.es/resoluciones/dificultades-para-concertar-cita-previa-para-solicitar-asilo/) y su obtención en tiempo y forma.

Si se encuentra con este problema, anote las fechas de sus intentos, los mensajes de error y las consultas al soporte. Describa su propia experiencia con precisión y preséntela por los canales oficiales. Las quejas documentadas ayudan a mostrar la magnitud del problema. No publique números de NIE, DNI o pasaporte, certificados, cookies ni otros datos personales.

## 🧰 Desarrollo

Rust, Tauri 2, Svelte 5 y TypeScript. SQLite y perfiles YAML compatibles. Control del navegador mediante CDP / WebDriver BiDi.

[Compilación y pruebas](docs/BUILD.md)

**💜 Apoyar el desarrollo**

Si la aplicación te ha ayudado, agradecería una donación para apoyar su desarrollo y mantenimiento.

[![Apoyar en GitHub](https://img.shields.io/badge/Apoyar%20en%20GitHub-ea4aaa?logo=githubsponsors&logoColor=white)](https://github.com/sponsors/itwormz)
[![Donar con criptomonedas](https://img.shields.io/badge/Donar%20con%20criptomonedas-5b5bd6)](https://nowpayments.io/donation/itworm)

## 💜 Agradecimientos

Gracias por ayudar a difundir el proyecto:

- <a href="https://t.me/bcn_today"><img src="docs/images/bcn_today.png" width="32" height="32" alt=""> BCN Today</a>
- <a href="https://t.me/spain_pass"><img src="docs/images/spain_pass.png" width="32" height="32" alt=""> Испанский Паспорт</a>

## 📄 Licencia

[PolyForm Noncommercial 1.0.0](LICENSE). Se permite el uso no comercial, la copia, la modificación y la distribución de la aplicación, conservando los términos de la licencia y el aviso de autoría. No se permite el uso comercial. Esta licencia se aplica al código de este proyecto; los componentes de terceros conservan sus propias licencias.
