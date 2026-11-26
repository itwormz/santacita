<p align="center">
  <img src="assets/santacita-icon.svg" width="144" height="144" alt="SantaCita logo">
</p>

<h1 align="center">SantaCita</h1>

<p align="center">
  An assistant for finding <b>Toma de huellas / TIE</b> and <b>Autorización de regreso</b> appointments on ICP Plus.<br>
  <sub>Free · Source available · macOS, Windows, Linux</sub>
</p>

<p align="center">

🇬🇧 **English** · 🇷🇺 [Русский](README.ru.md) · 🇪🇸 [Español](README.es.md)

</p>

<p align="center">

[![Latest published release](https://img.shields.io/github/v/release/itwormz/santacita?sort=semver&label=release&color=626bdf&logo=github)](https://github.com/itwormz/santacita/releases)

</p>

**💜 Support development**

If the app helped you, I would appreciate a donation to support its development and maintenance.

[![Support on GitHub](https://img.shields.io/badge/Support%20on%20GitHub-ea4aaa?logo=githubsponsors&logoColor=white)](https://github.com/sponsors/itwormz)
[![Donate with crypto](https://img.shields.io/badge/Donate%20with%20crypto-5b5bd6)](https://nowpayments.io/donation/itworm)

SantaCita is a desktop app that helps you get a cita for Toma de huellas / TIE and Autorización de regreso. It opens the booking site on its own, keeps watching it, enters your details when needed and takes you to the cita selection screen, staying within the site's limits. You do the date, time, CAPTCHA and booking confirmation yourself.

The app works with a browser already installed on your computer.

## 🔐 A certificate or Cl@ve is required

Right now it is practically impossible to get a cita without a digital certificate or [Cl@ve](https://clave.gob.es). If you have neither, get one first: a [personal certificate (FNMT)](https://www.sede.fnmt.gob.es/certificados/persona-fisica) or Cl@ve. The procedure is simple. The bot signs in to the site exactly this way.

## 🤖 How the bot works

The service strictly limits attempts: currently **5 cita requests per day per NIE**. After that the NIE is blocked and no cita can be obtained that day. So the bot does not try the form blindly and spends an attempt only when it can succeed:

1. Signs in with the certificate and reaches the data-entry page **without entering anything**.
2. Checks that page periodically and waits for the "no appointments" message to go away.
3. When the message is gone, **walks the whole path again** to the same page to make sure it was not a fluke.
4. If "no appointments" has not come back, **only then** enters your data and goes on to the cita selection.

This way none of the five daily requests is wasted on empty checks.

## 🚀 Quick start

1. Download the latest release for your system from [Releases](https://github.com/itwormz/santacita/releases) and install it (see "Installation" below).
2. Create a profile: province, service, sign-in method and applicant details.
3. Start the search and sign in on the site in the browser that opens.
4. When a cita appears, choose an office and finish the booking yourself. **Date, time, CAPTCHA and booking confirmation stay manual.**

## ✨ Features

- 👤 [Profiles](#-profiles) · 🌐 [Browsers](#-browsers)
- 🔎 Search: [how it runs](#-search) · 🎮 [controls](#-controls) · 🛡️ [block protection](#-block-protection)
- 📊 [Statistics and journal](#-statistics-and-journal) · 🔔 [Notifications](#-notifications) · 🩺 [Diagnostics](#-diagnostics)
- 🗂️ [Importing data](#-importing-data) · ⬆️ [Updates](#-updates)

### 👤 Profiles

A profile is one ready search: province, service, sign-in method (certificate or Cl@ve), applicant details, timings, protection and notifications. You can create several profiles and switch between them.

### 🌐 Browsers

SantaCita opens a separate profile in your browser: your everyday tabs and data are not touched.

- Windows: Chrome, Brave, Edge, Chromium, Firefox.
- macOS: Chrome, Brave, Chromium, Firefox.
- Linux: Chrome, Brave, Chromium, Firefox. Snap versions are not supported.

A new profile uses the first Chromium-based browser found (Chrome first), and Firefox when there is none.

### 🔎 Search

- The bot opens the site, signs in and reaches the applicant form, then waits for the "no appointments" message to go away, refreshing the page at the pauses you set.
- When a cita appears, it enters your details and goes on to the selection. It picks a single available office itself; with several offices it asks you to choose.
- Date, time, CAPTCHA and booking confirmation always stay with you.

How the bot reacts to "no appointments" is set in the profile:

- **Recheck availability** (on by default). When the message is gone, the bot walks the path again and enters your data only if the message has not returned. With it off, it enters the data at once.
- **Applicant form.** *Wait on "no appointments"* (default): data is not entered while the message is shown. *Enter and continue*: data is entered even with the message, and that empty check can use one of the five daily requests.
- **After "no appointments".** Start a new round or go back through Salir.

### 🎮 Controls

- Pause, resume, and return to the search after the date selection.
- The *Browser* button brings the browser window to the front.
- Timings and block protection apply immediately during a search; other profile changes apply on the next start.

### 🛡️ Block protection

- Typing has a human rhythm (the typing speed is set in the profile).
- Every session opens the browser window in its own size and place.
- If the site does not open or the session is rejected, the app restarts the browser with a clean profile and continues the search.

### 📊 Statistics and journal

Statistics, journal and achievements persist between runs. A confirmed booking can be added to a calendar, and the search statistics can be shared.

### 🔔 Notifications

The bot reports when your action is needed or the search has stopped.

**How it is delivered**

- 🔊 **Sound.** A signal on the computer when your action is needed or the search stopped with an error.
- ✈️ **Telegram.** A chat message: needs a bot token and recipients.
- 🔗 **Webhook.** A request to your HTTPS address; headers and a template are optional.

**What it reports** (for Telegram and webhook, events are chosen in the profile)

- **Sign-in:** you need to sign in on the site.
- **Office choice:** several offices, you have to choose.
- **Cita choice:** a cita is found, the rest is manual.
- **Search stopped:** the search stopped because of an error.
- **Unknown page:** the bot has stood on a page it does not recognize for a minute. Off by default; one message per page.

Secrets stay in the system credential store.

### 🩺 Diagnostics

The *Diagnostic report* collects information about how the app works, for troubleshooting. What it contains and how to remove personal data from it: [Diagnostics and privacy](docs/PRIVACY.md).

### 🗂️ Importing data

On first launch, SantaCita looks for the installed Python app and copies its profiles, statistics, achievements and journals. The original data stays in place; SantaCita keeps its own in a separate `SantaCita` folder. Importing again from Settings adds missing data and does not replace your current profiles.

Close the Python app before importing. You will have to sign in again in the browser. An old portable folder is not found automatically: keep its `data` folder and use a copy.

**🪟 Windows portable.** Extract the whole ZIP. The `portable.flag` file keeps data in the `data` folder next to the program. To bring old portable data over, copy the old `data` folder into the new portable folder before the first launch and keep the original.

### ⬆️ Updates

Checked in About, at startup and once a day. Download and installation start with a button. SHA256 and the Sigstore build provenance are checked before installing; a package that fails the check is not installed. Windows portable replaces the program and restarts, keeping `data`.

The *Offer beta versions* setting adds the betas of later versions (0.5.0 and above) to the stable ones. It is off by default and imported from Python 0.3.8.

## 📥 Installation

SantaCita runs on macOS (Apple Silicon and Intel), Windows 10 and 11 (x64) and Linux (x64).

| System | File |
| --- | --- |
| macOS Apple Silicon | `SantaCita-<version>-macOS-Apple-Silicon.dmg` |
| macOS Intel | `SantaCita-<version>-macOS-Intel.dmg` |
| Windows x64 | `SantaCita-<version>-windows-x64.msi` or `-portable.zip` |
| Linux x64 | `SantaCita-<version>-x86_64.AppImage` |

**🍎 macOS.** Open the DMG and drag SantaCita into Applications. The app is not notarized, so if it is blocked, open System Settings → Privacy & Security → Open Anyway.

**🪟 Windows.** Run the MSI (it installs WebView2 itself if needed) or extract the whole portable ZIP. SmartScreen may require More info → Run anyway.

**🐧 Linux.** Make the AppImage executable (`chmod +x file.AppImage`) and run it. It needs `fuse3`; if it is missing, install it (`sudo apt install fuse3`, `sudo dnf install fuse3`, `sudo pacman -S fuse3`) or start the file with `--appimage-extract-and-run`. The browser must not be a snap: use Chrome from its `.deb` package or Firefox from Mozilla's APT repository.

## 🕰️ If the new version does not suit you

Versions before 0.4.0 were written in Python and shipped with their own Camoufox browser; their code stays in the [`ceh-python`](https://github.com/itwormz/santacita/tree/ceh-python) branch. If 0.4 and later does not work with your browser, you can temporarily use [0.3.8](https://github.com/itwormz/santacita/releases/tag/v0.3.8).

## ⚠️ Limitations

**🌍 All systems**

- An installed browser is required; notifications need the system credential store.
- Website errors, HTTP 429 restrictions and reauthentication can occur. An appointment is not guaranteed.
- [Diagnostics and privacy](docs/PRIVACY.md). Remove personal data before sharing reports.

**🍎 macOS.** The system may ask for the Keychain password when the app needs the Telegram token.

**🪟 Windows.** Uses WebView2. The MSI installs it itself (needs network access; running the MSI again repairs a failed attempt). For the portable ZIP, install [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/).

**🐧 Linux.** Requires a graphical desktop, glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 and newer), the Mesa GL/EGL libraries and a Secret Service keyring (gnome-keyring or KWallet). Tested in containers on Ubuntu 22.04, Fedora 37, Kali and Arch; real GPU, Wayland, tray and notifications are not verified yet. Under native Wayland, you may need to open the browser window manually.

## 🎯 Purpose

An independent project for obtaining your own appointment, not affiliated with Spanish authorities. Not intended for appointment resale or mass capture.

Provided as is, without a guarantee of operation or an appointment. Users are responsible for following the law and the service rules.

## 📣 A message to the responsible authorities

Access to a mandatory administrative procedure should not depend on a person's ability to refresh a page for weeks, guess when appointments become available or resort to paid intermediaries.

In my view, a system that requires foreign nationals to do this undermines their dignity and creates a practical barrier to public services. A transparent queue, clear waiting times, availability notifications and an accessible alternative to online booking are needed.

Measures against automation can also hinder ordinary users. Service protection should account for false positives and provide a clear process for restoring access. A person trying to complete a mandatory administrative procedure should not have to understand the technical workings of anti-bot protection.

This project exists because the ordinary booking process requires disproportionate effort. I hope improvements to the official service will make assistants like this unnecessary.

### The shadow market and its public cost

An opaque booking process and difficulties with ordinary access create favourable conditions for a shadow market in which access to a free public service becomes a commodity. Spain's Ministry of the Interior has reported investigations into schemes using automation to capture appointments in bulk for resale. Such schemes further hinder people seeking their own appointment.

The consequences extend beyond inconvenience for foreign nationals: combating appointment hoarding and resale requires police investigations, technical expert analysis and judicial involvement. In my view, an accessible and transparent allocation system should address the conditions that enable this market alongside enforcement against abuse.

Public appointments should be allocated according to clear and fair rules, rather than becoming a competition in technical capabilities or applicants' ability to pay.

Official examples of investigations:

- **12 May 2023:** the Ministry reported 69 arrests and a further 25 people under investigation on suspicion of involvement in a criminal organization and computer damage. The investigation involved Investigating Court No. 8 in Valencia; according to police, appointments were captured for resale at €30–200. [Ministry statement](https://www.interior.gob.es/opencms/es/detalle/articulo/La-Policia-Nacional-desmantela-un-entramado-que-bloqueaba-el-sistema-de-citas-online-de-extranjeria-mediante-un-bot-informatico/).
- **27 November 2024:** the Ministry reported 21 arrests in Alicante, Murcia and Valencia in an investigation involving appointment sales, blocking the system with bots and document forgery. [Ministry statement](https://www.interior.gob.es/opencms/es/detalle/articulo/La-Policia-Nacional-desarticula-un-grupo-criminal-dedicado-a-la-venta-fraudulenta-de-citas-de-extranjeria-mediante-bots/).

These are reports of investigations and arrests, not confirmation of final criminal convictions.

### Your rights and where to seek help

- **Legal basis:** Articles 12 and 13 of [Ley 39/2015](https://www.boe.es/eli/es/l/2015/10/01/39) provide for assistance with electronic services and respectful treatment that facilitates exercising rights and fulfilling obligations. These provisions do not authorize automation.
- **Technical errors and blocks:** the [official appointment support page](https://sede.administracionespublicas.gob.es/pagina/index/directorio/ayuda_cita_previa) lists telephone 060 and a technical enquiry form. This channel handles technical issues.
- **Complaints about access to public services:** complaints to the Defensor del Pueblo are free, available to foreign nationals regardless of their legal status and do not require a lawyer. [Who can complain and how](https://www.defensordelpueblo.es/el-defensor/como-te-podemos-ayudar/) · [Submit a complaint](https://portal-sac.defensordelpueblo.es/es/form/queja). Complaints do not suspend administrative deadlines.
- **Official recommendations:** the Defensor del Pueblo has already [called for improved access to appointments](https://www.defensordelpueblo.es/resoluciones/dificultades-para-concertar-cita-previa-para-solicitar-asilo/) and timely booking.

If you face the same problem, record the dates of attempts, error messages and support enquiries. Describe your own experience accurately and submit it through official channels. Documented complaints help show the scale of the problem. Do not publish NIE, DNI or passport numbers, certificates, cookies or other personal data.

## 🧰 Development

Rust, Tauri 2, Svelte 5 and TypeScript. SQLite and compatible YAML profiles. Browser control through CDP / WebDriver BiDi.

[Build and checks](docs/BUILD.md)

**Developer window.** Three clicks on the small wrench in the bottom-right corner of the About window open it. It turns on a recording of the pages a search passes through, also while you drive the browser by hand: one file per page (its structure, forms and visible text) and the path between pages, in the `data/learning` folder. Typed values, profile data, phones, emails and DNI/NIE are blanked. `CEH_LEARNING=<folder>` at launch still turns the recording on. The window also shows what the bot is doing, opens the app's folders, exports the last session as a zip and checks whether the site's start page opens. The diagnostics read every page of the site, whatever its path (Madrid uses `icpplustiem`).

## 💜 Acknowledgements

Thank you for helping spread the word about the project:

- <a href="https://t.me/bcn_today"><img src="docs/images/bcn_today.png" width="20" height="20" alt=""> BCN Today</a>
- <a href="https://t.me/spain_pass"><img src="docs/images/spain_pass.png" width="20" height="20" alt=""> Испанский Паспорт</a>

## 📄 License

[PolyForm Noncommercial 1.0.0](LICENSE). Noncommercial use, copying, modification and distribution are permitted, provided the license terms and copyright notice are preserved. Commercial use is not permitted. This license applies to this project’s code; third-party components retain their own licenses.
