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

**💜 Apoyar el desarrollo**

Si la aplicación te ha ayudado, agradecería una donación para apoyar su desarrollo y mantenimiento.

[![Apoyar en GitHub](https://img.shields.io/badge/Apoyar%20en%20GitHub-ea4aaa?logo=githubsponsors&logoColor=white)](https://github.com/sponsors/itwormz)
[![Donar con criptomonedas](https://img.shields.io/badge/Donar%20con%20criptomonedas-5b5bd6)](https://nowpayments.io/donation/itworm)

SantaCita es una aplicación de escritorio que ayuda a conseguir cita para Toma de huellas / TIE y Autorización de regreso. Abre sola la web de citas, la vigila, introduce sus datos cuando hace falta y le lleva hasta la pantalla de selección de cita, respetando los límites del sitio. La fecha, la hora, el CAPTCHA y la confirmación de la reserva los hace usted.

La aplicación funciona con el navegador que ya tiene instalado en el ordenador.

## 🔐 Se necesita certificado o Cl@ve

Hoy es prácticamente imposible conseguir una cita sin certificado digital o [Cl@ve](https://clave.gob.es). Si no tiene ninguno, obtenga primero uno: [certificado de persona física (FNMT)](https://www.sede.fnmt.gob.es/certificados/persona-fisica) o Cl@ve. El trámite es sencillo. El bot accede a la web precisamente de esta forma.

## 🤖 Cómo funciona el bot

El servicio limita estrictamente los intentos: actualmente **5 solicitudes de cita al día por NIE**. Después el NIE queda bloqueado y ese día ya no se puede conseguir cita. Por eso el bot no prueba el formulario a ciegas y gasta un intento solo cuando puede funcionar:

1. Accede con el certificado y llega a la página de datos **sin introducir nada**.
2. Revisa esa página periódicamente y espera a que desaparezca el mensaje de que no hay citas.
3. Cuando el mensaje desaparece, **repite todo el recorrido** hasta la misma página para comprobar que no fue algo puntual.
4. Si el mensaje de que no hay citas no ha vuelto, **solo entonces** introduce sus datos y continúa hasta la selección de cita.

Así ninguna de las cinco solicitudes diarias se gasta en comprobaciones vacías.

## 🚀 Inicio rápido

1. Descargue la última versión para su sistema desde [Releases](https://github.com/itwormz/santacita/releases) e instálela (véase «Instalación» más abajo).
2. Cree un perfil: provincia, trámite, forma de acceso y datos del solicitante.
3. Inicie la búsqueda y acceda a la web en el navegador que se abre.
4. Cuando aparezca una cita, elija oficina y termine la reserva usted mismo. **Fecha, hora, CAPTCHA y confirmación de la reserva siguen siendo manuales.**

## ✨ Funciones

- 👤 [Perfiles](#-perfiles) · 🌐 [Navegadores](#-navegadores)
- 🔎 Búsqueda: [cómo funciona](#-búsqueda) · 🎮 [control](#-control) · 🛡️ [protección contra bloqueos](#-protección-contra-bloqueos)
- 📊 [Estadísticas y registro](#-estadísticas-y-registro) · 🔔 [Notificaciones](#-notificaciones) · 🩺 [Diagnóstico](#-diagnóstico)
- 🗂️ [Importar datos](#-importar-datos) · ⬆️ [Actualizaciones](#-actualizaciones)

### 👤 Perfiles

Un perfil es una búsqueda ya preparada: provincia, trámite, forma de acceso (certificado o Cl@ve), datos del solicitante, tiempos, protección y notificaciones. Puede crear varios perfiles y cambiar entre ellos.

### 🌐 Navegadores

SantaCita abre un perfil separado en su navegador: sus pestañas y datos habituales no se tocan.

- Windows: Chrome, Brave, Edge, Chromium, Firefox.
- macOS: Chrome, Brave, Chromium, Firefox.
- Linux: Chrome, Brave, Chromium, Firefox. No se admiten las versiones snap.

Un perfil nuevo usa el primer navegador basado en Chromium que se encuentre (primero Chrome) y Firefox si no hay ninguno.

### 🔎 Búsqueda

- El bot abre la web, accede y llega al formulario del solicitante; después espera a que desaparezca el mensaje de que no hay citas y actualiza la página con las pausas que haya fijado.
- Cuando aparece una cita, introduce sus datos y sigue hasta la selección. Si hay una sola oficina disponible, la elige él; si hay varias, le pide que elija.
- La fecha, la hora, el CAPTCHA y la confirmación de la reserva siempre quedan en sus manos.

Cómo reacciona el bot al mensaje de que no hay citas se ajusta en el perfil:

- **Volver a comprobar la disponibilidad** (activado por defecto). Cuando el mensaje desaparece, el bot repite el recorrido y introduce los datos solo si el mensaje no ha vuelto. Si se desactiva, los introduce enseguida.
- **En el formulario del solicitante.** *Esperar con «no hay citas»* (por defecto): no se introducen datos mientras se vea el mensaje. *Introducir y continuar*: se introducen aunque el mensaje siga visible, y esa comprobación vacía puede gastar una de las cinco solicitudes diarias.
- **Tras «no hay citas».** Empezar una nueva ronda o volver mediante Salir.

### 🎮 Control

- Pausar, reanudar y volver a la búsqueda después de elegir la fecha.
- El botón *Navegador* trae la ventana del navegador al primer plano.
- Los tiempos y la protección contra bloqueos se aplican al instante durante la búsqueda; los demás cambios del perfil, en el siguiente inicio.

### 🛡️ Protección contra bloqueos

- La escritura sigue un ritmo humano (la velocidad se fija en el perfil).
- Cada sesión abre la ventana del navegador con su propio tamaño y posición.
- Si la web no abre o se rechaza la sesión, la aplicación reinicia el navegador con un perfil limpio y continúa la búsqueda.

### 📊 Estadísticas y registro

Las estadísticas, el registro y los logros se conservan entre búsquedas. Puede añadir una reserva confirmada al calendario y compartir las estadísticas de la búsqueda.

### 🔔 Notificaciones

El bot avisa cuando hace falta su acción o la búsqueda se ha detenido.

**Cómo se entrega**

- 🔊 **Sonido.** Una señal en el ordenador cuando hace falta su acción o la búsqueda se detuvo con un error.
- ✈️ **Telegram.** Un mensaje en el chat: requiere token de bot y destinatarios.
- 🔗 **Webhook.** Una petición a su dirección HTTPS; las cabeceras y la plantilla son opcionales.

**De qué avisa** (en Telegram y webhook, los eventos se eligen en el perfil)

- **Acceso:** hay que acceder a la web.
- **Elección de oficina:** hay varias y debe elegir usted.
- **Elección de cita:** se ha encontrado una cita, el resto es manual.
- **Búsqueda detenida:** la búsqueda se detuvo por un error.
- **Página desconocida:** el bot lleva un minuto en una página que no reconoce. Desactivado por defecto; un mensaje por página.

Los secretos se guardan en el almacén de credenciales del sistema.

### 🩺 Diagnóstico

El *informe de diagnóstico* reúne información sobre el funcionamiento de la aplicación para resolver problemas. Qué contiene y cómo quitarle los datos personales: [Diagnóstico y privacidad](docs/PRIVACY.md).

### 🗂️ Importar datos

Al iniciarse por primera vez, SantaCita busca la aplicación Python instalada y copia sus perfiles, estadísticas, logros y registros. Los originales se conservan; SantaCita guarda los suyos en una carpeta `SantaCita` independiente. Importar de nuevo desde Ajustes añade los datos que falten y no sustituye sus perfiles actuales.

Cierre Python antes de importar. Tendrá que acceder de nuevo en el navegador. La carpeta portable antigua no se detecta automáticamente: conserve su carpeta `data` y use una copia.

**🪟 Windows portable.** Extraiga el ZIP completo. El archivo `portable.flag` guarda los datos en la carpeta `data` junto al programa. Para traer datos portable anteriores, copie la antigua carpeta `data` a la nueva carpeta portable antes del primer inicio y conserve el original.

### ⬆️ Actualizaciones

Se comprueban en Acerca de, al iniciar y una vez al día. La descarga y la instalación se inician con un botón. Antes de instalar se verifican SHA256 y la procedencia Sigstore de la compilación; un paquete que no la supera no se instala. Windows portable sustituye el programa y se reinicia conservando `data`.

El ajuste *Ofrecer versiones beta* añade a las estables las betas de versiones posteriores (0.5.0 y superiores). Está desactivado por defecto y se importa desde Python 0.3.8.

## 📥 Instalación

SantaCita funciona en macOS (Apple Silicon e Intel), Windows 10 y 11 (x64) y Linux (x64).

| Sistema | Archivo |
| --- | --- |
| macOS Apple Silicon | `SantaCita-<versión>-macOS-Apple-Silicon.dmg` |
| macOS Intel | `SantaCita-<versión>-macOS-Intel.dmg` |
| Windows x64 | `SantaCita-<versión>-windows-x64.msi` o `-portable.zip` |
| Linux x64 | `SantaCita-<versión>-x86_64.AppImage` |

**🍎 macOS.** Abra el DMG y arrastre SantaCita a Aplicaciones. La aplicación no está notarizada; si se bloquea, abra Ajustes del Sistema → Privacidad y seguridad → Abrir igualmente.

**🪟 Windows.** Ejecute el MSI (instala WebView2 por sí mismo si hace falta) o extraiga el ZIP portable completo. SmartScreen puede requerir Más información → Ejecutar de todas formas.

**🐧 Linux.** Dé permiso de ejecución al AppImage (`chmod +x archivo.AppImage`) y ejecútelo. Necesita `fuse3`; si falta, instálelo (`sudo apt install fuse3`, `sudo dnf install fuse3`, `sudo pacman -S fuse3`) o inicie el archivo con `--appimage-extract-and-run`. El navegador no debe ser un snap: use Chrome desde su paquete `.deb` o Firefox desde el repositorio APT de Mozilla.

## 🕰️ Si la versión nueva no le sirve

Las versiones anteriores a la 0.4.0 estaban escritas en Python e incluían su propio navegador Camoufox; su código permanece en la rama [`ceh-python`](https://github.com/itwormz/santacita/tree/ceh-python). Si la 0.4 y posteriores no funcionan con su navegador, puede usar temporalmente la [0.3.8](https://github.com/itwormz/santacita/releases/tag/v0.3.8).

## ⚠️ Limitaciones

**🌍 Todos los sistemas**

- Hace falta un navegador instalado; las notificaciones necesitan el almacén de credenciales del sistema.
- Pueden producirse errores del sitio, límites HTTP 429 y nuevas autenticaciones. No se garantiza obtener cita.
- [Diagnóstico y privacidad](docs/PRIVACY.md). Elimine los datos personales antes de compartir informes.

**🍎 macOS.** El sistema puede pedir la contraseña del Llavero cuando la aplicación necesita el token de Telegram.

**🪟 Windows.** Usa WebView2. El MSI lo instala por sí mismo (hace falta acceso a la red; volver a ejecutar el MSI repara un intento fallido). Para el ZIP portable, instale [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/).

**🐧 Linux.** Requiere escritorio gráfico, glibc 2.35 o superior (Ubuntu 22.04, Debian 12, Fedora 36 y posteriores), las bibliotecas Mesa GL/EGL y un llavero Secret Service (gnome-keyring o KWallet). Probado en contenedores con Ubuntu 22.04, Fedora 37, Kali y Arch; la GPU real, Wayland, la bandeja y las notificaciones aún no están verificados. En Wayland nativo puede ser necesario abrir la ventana del navegador manualmente.

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

**Modo de aprendizaje (para desarrolladores).** Inicie la aplicación con `CEH_LEARNING=<carpeta>` y registrará las páginas por las que pasa la búsqueda, también cuando usted maneja el navegador a mano: un archivo por página (estructura, formularios y texto visible) y el recorrido entre páginas. Los valores escritos, los datos del perfil, teléfonos, correos y DNI/NIE se tachan. Sin la variable no se registra nada. El diagnóstico lee todas las páginas del sitio, sea cual sea su ruta (Madrid usa `icpplustiem`).

## 💜 Agradecimientos

Gracias por ayudar a difundir el proyecto:

- <a href="https://t.me/bcn_today"><img src="docs/images/bcn_today.png" width="20" height="20" alt=""> BCN Today</a>
- <a href="https://t.me/spain_pass"><img src="docs/images/spain_pass.png" width="20" height="20" alt=""> Испанский Паспорт</a>

## 📄 Licencia

[PolyForm Noncommercial 1.0.0](LICENSE). Se permite el uso no comercial, la copia, la modificación y la distribución de la aplicación, conservando los términos de la licencia y el aviso de autoría. No se permite el uso comercial. Esta licencia se aplica al código de este proyecto; los componentes de terceros conservan sus propias licencias.
