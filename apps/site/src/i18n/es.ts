import { REPO_URL } from "../data/release";
import type { Messages } from "./types";

export const es: Messages = {
  lang: "es",
  language: "Idioma",
  description:
    "Almacenamiento inteligente para todos tus dispositivos. Las reglas de seguridad y la revisión opcional con IA te ayudan a entender qué ocupa espacio antes de decidir qué quitar. Lumen es un programa en fase prealfa.",
  ogDescription:
    "La seguridad de tus archivos importa más que liberar espacio. Código abierto, local y en fase prealfa.",
  skip: "Saltar al contenido",
  nav: { label: "Sitio", docs: "Documentación" },
  hero: {
    headline: ["Storage intelligence", "for every device."],
    lead: "Qué conservar y qué quitar: la IA te ayuda a decidir.",
    download: "Ver versiones",
    requirements: "macOS 13+ · Windows 10 22H2+ · Linux x86_64",
  },
  showcase: {
    main: "Página Explorar de Lumen con datos de ejemplo: espacio por decisión, 9,9 GB propuestos entre herramientas de desarrollo y cachés de aplicaciones, y comandos para que cada herramienta limpie sus propios elementos.",
  },
  features: {
    title: "Qué hace Lumen",
    items: [
      {
        title: "Encuentra qué ocupa espacio",
        text: "Desde datos de compilación de Xcode y cachés de Gradle hasta cachés de aplicaciones e instaladores antiguos. El análisis nunca abre archivos.",
      },
      {
        title: "Explica cada decisión",
        text: "Quién creó un elemento, si se puede volver a crear y si algo lo sigue usando: las pruebas determinan si conviene conservarlo, revisarlo o limpiarlo.",
      },
      {
        title: "La IA vuelve a comprobarlo",
        text: "La primera vez que se inicia, Lumen pregunta si quieres usar la revisión con IA y no envía nada hasta que eliges. Si la activas, las rutas, los tamaños y la antigüedad de los elementos se envían al servidor Jeb de una escuela; quien lo administra podría guardarlos. El contenido de los archivos nunca sale de tu dispositivo. La IA vuelve a revisar lo que Lumen propone y añade una nota sobre qué podrían ser los elementos que requieren revisión. Solo puede hacer que una decisión sea más prudente. Puedes cambiar tu elección en Ajustes.",
      },
      {
        title: "Limpia solo cuando lo confirmas",
        text: "Los archivos se mueven a Cuarentena, donde puedes restaurarlos. Para el almacén de pnpm o los simuladores de Xcode, revisa y ejecuta el comando de limpieza de la propia herramienta. Esos comandos eliminan datos directamente y no se pueden deshacer.",
      },
    ],
  },
  trust: {
    items: [
      {
        headline: ["Primero, tus archivos.", "Después, el espacio libre."],
        text: "Lumen no elimina nada automáticamente. Puedes restaurar los archivos en cuarentena hasta que los elimines definitivamente.",
      },
      {
        headline: ["Consulta las pruebas.", "No dejes nada al azar."],
        text: "Qué es un archivo y qué lo sigue usando. Si no está claro, se queda donde está.",
      },
    ],
    more: "Leer la arquitectura",
  },
  download: {
    title: "Descargas e instalación",
    noScript: [
      "Los archivos están en ",
      { link: "las versiones de GitHub", href: `${REPO_URL}/releases` },
      ".",
    ],
    checking: "Comprobando versiones…",
    thisComputer: "<-- Tienes que descargar",
    installer: "Instalador",
    requirements: {
      mac: "macOS 13 o posterior · Apple silicon e Intel",
      windows:
        "Windows 10 22H2 o posterior, Windows 11 · 64 bits · no necesitas permisos de administrador",
      linux: "x86_64 · Ubuntu 24.04 o posterior, o una distribución similar",
    },
  },
  files: {
    summary: "Otros archivos: línea de comandos, sumas de comprobación y SBOM",
    intro: [
      "La herramienta de línea de comandos ",
      { code: "lumen" },
      " está disponible como ",
      { code: "lumen-<target>.tar.gz" },
      " (macOS con Apple silicon y Linux) y ",
      { code: ".zip" },
      " (Windows). Los archivos ",
      { code: ".cdx.json" },
      " enumeran los componentes de cada compilación (SBOM).",
    ],
  },
  verify: {
    summary: "Verificar una descarga",
    introBefore: ["Pon "],
    introAfter: [
      " junto a la descarga y ejecuta este comando en esa carpeta. El resultado del archivo debe ser ",
      { code: "OK" },
      ".",
    ],
    windowsNote:
      " · compara el resultado con la línea de ese archivo en SHA256SUMS; no importa si cambia entre mayúsculas y minúsculas.",
    provenance: "Procedencia de la compilación",
    provenanceNote: [
      " · comprueba que el flujo de versiones de este repositorio creó el archivo. Necesitas ",
      { link: "GitHub CLI", href: "https://cli.github.com/" },
      ".",
    ],
  },
  install: {
    title: "Abrir Lumen por primera vez",
    intro:
      "Lumen aún no está firmado, así que macOS y Windows bloquean el primer inicio. Autorízalo una vez y luego se abrirá con normalidad.",
    mac: {
      steps: [
        ["Abre el archivo ", { code: ".dmg" }, " y arrastra Lumen a ", { ui: "Aplicaciones" }, "."],
        ["Abre Lumen. Cuando macOS indique que no se puede abrir, pulsa ", { ui: "Hecho" }, "."],
        [
          "En ",
          { ui: "Ajustes del Sistema › Privacidad y seguridad" },
          ", baja hasta ",
          { ui: "Seguridad" },
          ", pulsa ",
          { ui: "«Abrir igualmente»" },
          " y confirma con tu contraseña o Touch ID.",
        ],
        [
          "Abre Lumen otra vez y pulsa ",
          { ui: "Abrir igualmente" },
          ". Después se abrirá con normalidad.",
        ],
      ],
      aside: [
        "En macOS 13 y 14 también puedes hacer clic mientras mantienes Control y elegir ",
        { ui: "Abrir" },
        ". En macOS 15 o posterior, sigue los pasos anteriores.",
      ],
    },
    fda: {
      summary: "Acceso total al disco en macOS",
      intro:
        "Con este permiso, Lumen también puede analizar carpetas protegidas, como los datos de Mail, Mensajes y otras aplicaciones. Sin él, esas carpetas no se pueden leer y Lumen las deja intactas.",
      steps: [
        [
          "Abre ",
          { ui: "Ajustes del Sistema › Privacidad y seguridad › Acceso total al disco" },
          " o usa el botón de Ajustes de Lumen.",
        ],
        [
          "Activa Lumen. Si no aparece en la lista, añádelo desde Aplicaciones con ",
          { ui: "+" },
          ".",
        ],
        ["Cierra y vuelve a abrir Lumen; después, analiza de nuevo."],
      ],
      aside: [
        "Como la aplicación aún no está firmada, macOS puede considerar que una versión actualizada es otra aplicación. Si después de actualizar no se pueden leer las carpetas, quita Lumen de la lista con ",
        { ui: "−" },
        " y vuelve a añadirlo.",
      ],
    },
    windows: {
      steps: [
        [
          "Si el navegador muestra una advertencia, elige ",
          { ui: "Conservar" },
          " en el menú de descargas.",
        ],
        [
          "Ejecuta el instalador. En ",
          { ui: "Windows protegió tu PC" },
          ", pulsa ",
          { ui: "Más información" },
          " y luego ",
          { ui: "Ejecutar de todas formas" },
          ".",
        ],
        [
          "Abre Lumen desde el menú Inicio como usuario normal. Si lo ejecutas como administrador, Lumen no moverá archivos.",
        ],
      ],
    },
    linux: {
      aside: [
        "Si AppImage no se inicia, quizá necesites FUSE 2 (Ubuntu 24.04: ",
        { code: "sudo apt install libfuse2t64" },
        "). Ejecuta Lumen como usuario normal; como root, no moverá archivos.",
      ],
    },
  },
  footer: { source: "Repositorio de GitHub", releases: "Todas las versiones", contact: "Contacto" },
  copy: "Copiar",
  client: {
    dateLocale: "es-ES",
    download: "Elige una descarga para escritorio",
    downloadFor: "Descargar para {platform}",
    viewOnGitHub: "Seguir el desarrollo en GitHub",
    downloadFromGitHub: "Consultar las versiones de GitHub",
    getFromGitHub: "Consultar las versiones de GitHub",
    comingSoonShort: "Todavía no hay una versión pública. ",
    loadFailed: "No se puede consultar la información de versiones. Compruébala en GitHub.",
    checkFailed: "No se puede consultar la información de versiones. ",
    rateLimited:
      "GitHub ha recibido demasiadas solicitudes y no se pudo comprobar la última versión. ",
    releaseNotes: "Notas de la versión",
    githubReleases: "Versiones de GitHub",
    allFiles: "Ver todos los archivos en GitHub",
    fileHeader: "Archivo",
    sizeHeader: "Tamaño",
    copied: "Copiado",
  },
  notFound: { title: "No se encontró la página", home: "Ir a la página de descarga de Lumen" },
};
