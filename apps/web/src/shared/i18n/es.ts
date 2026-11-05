import type { Messages } from "./en";
import { countFormat } from "./format";

const count = countFormat("es");

/** «1 elemento», «1.204 elementos». */
const items = (n: number) => `${count(n)} ${n === 1 ? "elemento" : "elementos"}`;

/** «1 elemento más», «3 elementos más». */
const others = (n: number) => `${count(n)} ${n === 1 ? "elemento más" : "elementos más"}`;

/** «Spotify y Discord»; con más de tres nombres, «A, B, C y 2 más». */
function appList(apps: readonly string[]): string {
  if (apps.length > 3) {
    return `${apps.slice(0, 3).join(", ")} y ${count(apps.length - 3)} más`;
  }
  return new Intl.ListFormat("es", { type: "conjunction" }).format(apps);
}

/** Catálogo en español. Mantiene la misma forma que el catálogo inglés. */
export const es: Messages = {
  language: "Idioma",
  nav: {
    label: "Principal",
    overview: "Almacenamiento",
    cleanUp: "Explorar",
    quarantine: "Cuarentena",
    settings: "Ajustes",
    held: (n: number) => `${items(n)} en Cuarentena`,
  },
  common: {
    loading: "Cargando…",
    close: "Cerrar",
    details: "Detalles",
    noDetails: "Lumen no obtuvo detalles sobre este error.",
    tryAgain: "Volver a intentar",
    scanDisk: "Analizar disco",
    scanning: "Analizando…",
    scanAgain: "Analizar de nuevo",
    scanOther: "Analizar otras carpetas",
    items: (n: number) => items(n),
  },
  overview: {
    title: "Almacenamiento",
    disk: {
      title: "Disco de arranque",
      meter: "Uso del disco",
      used: (size: string) => `${size} usados`,
      total: (size: string) => `${size} en total`,
      percentUsed: (percent: string) => `${percent}% usado`,
      assessed: (size: string) => `Evaluado: ${size}`,
      low: "Casi lleno: queda menos del 10 % disponible.",
      loadFailed: "Lumen no pudo leer el uso del disco.",
    },
    session: {
      title: "Último análisis",
      quarantine: "En Cuarentena",
      atLeast: (size: string) => `al menos ${size}`,
      found: (n: string) => `${n} leídos`,
      openExplore: "Abrir Explorar",
      showProgress: "Mostrar progreso",
      cleaned: (n: number, size: string) => `${items(n)} (${size}) se movieron a Cuarentena.`,
      none: "Todavía no se ha analizado nada. Analiza el disco para explorar su contenido y las pruebas que respaldan cada decisión.",
    },
  },
  workspace: {
    available: "Espacio disponible",
    assessed: "Elementos evaluados · por tamaño",
    assessment: (n: number) => `${items(n)} evaluados`,
    unknownOrigin: "No hay información sobre el origen",
    workflow: "Del análisis a la limpieza",
    scanStage: "Analizar",
    resultsStage: "Ver resultados",
    cleanUpStage: "Limpiar",
    activeCommand: "Comando en ejecución",
    quitHeld: "Lumen está moviendo o eliminando elementos. Puedes cerrarlo cuando termine.",
  },
  start: {
    title: "Descubre qué hay en tu disco",
    promise: "El análisis solo lee y dura unos minutos. Lumen te pregunta antes de mover nada.",
    wholeDiskHint: "Es posible que el ordenador te pida permiso para acceder a algunas carpetas.",
    chooseFolders: "Analizar solo algunas carpetas",
    foldersLabel: "Carpetas que se analizarán, una por línea",
    foldersEmpty: "Introduce al menos una carpeta.",
    scanFolders: "Analizar estas carpetas",
    failed: "No se pudo completar el análisis. Tus archivos no han cambiado.",
    aiOn: (service: string) =>
      `La revisión con IA está activada: Lumen envía al servidor de ${service} los detalles de los elementos propuestos y de algunos que requieren revisión, nunca su contenido. Puedes desactivarla en Ajustes.`,
  },
  ai: {
    title: "Revisión con IA",
    offIntro:
      "Un modelo de IA vuelve a examinar lo que Lumen propone limpiar y te orienta sobre qué podrían ser los elementos que requieren revisión. Solo puede hacer que Lumen actúe con más cautela.",
    onIntro: (key: string) =>
      `Activada con ${key}. Lumen envía los detalles de cada elemento propuesto y de hasta 20 elementos que requieren revisión, nunca su contenido. Solo puede hacer que Lumen actúe con más cautela. Puedes desactivarla cuando quieras.`,
    keyInUse: {
      builtin: (service: string) => `${service} (clave integrada)`,
      stored: (service: string) => `${service} (tu clave)`,
    },
    checking: "Comprobando…",
    short: {
      on: "Revisión con IA activada",
      off: "Revisión con IA desactivada",
    },
    loadFailed: "Lumen no pudo cargar el ajuste de revisión con IA.",
    changeFailed: "Lumen no pudo cambiar el ajuste de revisión con IA.",
    consent: {
      title: "¿Activar la revisión con IA?",
      what: "Por cada elemento propuesto y hasta 20 de los elementos más grandes que requieren revisión, Lumen envía su ruta (mostrando tu carpeta de inicio como ~), tamaño, antigüedad y las pruebas que respaldan la decisión. Nunca se envía el contenido de los archivos.",
      whereJeb:
        "Lumen lo envía con tu clave de API al servidor Jeb de tu escuela (jeb.https.gsmsv.site). Quien opera el servidor podría conservar las solicitudes.",
      whereJebBuiltin:
        "Lumen lo envía con su clave integrada al servidor Jeb de una escuela (jeb.https.gsmsv.site). Quien opera el servidor podría conservar las solicitudes.",
      whereOpenRouter:
        "Lumen lo envía con tu clave de API a Jev de TypeSafe a través de OpenRouter.",
      cautious:
        "La IA solo puede hacer que una decisión sea más cautelosa. Para los elementos que requieren revisión, solo añade una nota sobre qué podría ser cada uno. Nunca elige qué se mueve o se elimina.",
      revocable:
        "A partir de entonces, la revisión con IA se ejecuta en cada análisis. Puedes desactivarla en Ajustes cuando quieras.",
      keyLabel: "Clave de API de Jeb",
      keyHint: "Lumen la guarda en el almacenamiento seguro del sistema y no vuelve a mostrarla.",
      keyInvalid: "Pega la clave completa: entre 1 y 256 caracteres, sin espacios.",
      ownKey: "Usar tu propia clave",
      ownKeyHint:
        "Opcional. Lumen la usa en lugar de la clave integrada, la guarda en el almacenamiento seguro del sistema y no vuelve a mostrarla.",
      confirm: "Activar la revisión con IA",
      turningOn: "Activando…",
    },
    firstLaunch: {
      title: "¿Usar la revisión con IA?",
      intro:
        "Lumen decide según sus propias reglas de seguridad. Si quieres, un modelo de IA puede volver a examinar los elementos. No se envía nada hasta que tú lo eliges.",
      on: "Activar la revisión con IA",
      off: "Continuar sin revisión con IA",
    },
  },
  scanning: {
    title: "Analizando…",
    deciding: "Tomando decisiones…",
    decidingHint: "Lumen está averiguando qué es cada elemento y qué hacer con él. No cambia nada.",
    stages: {
      inventory: "Comprobando las aplicaciones instaladas…",
      markers: "Comprobando de dónde vienen los elementos…",
      evidence: "Reconociendo cachés y carpetas de herramientas…",
      graph: "Relacionando los elementos con sus aplicaciones y herramientas…",
      deciding: "Aplicando las reglas de seguridad…",
      describing: "La revisión con IA está describiendo los elementos que requieren revisión…",
    },
    stagePercent: (n: number) => `${n}% de esta etapa`,
    processed: (done: string, total: string) => `${done} de ${total} elementos comprobados`,
    readOnly: "Lumen solo lee; no cambia nada.",
    progress: "Progreso del análisis",
    items: "Entradas leídas",
    unreadable: "Carpetas ilegibles",
    reading: "Leyendo",
    readSoFar: (size: string) => `Leído hasta ahora: ${size}`,
    failed: "Lumen no pudo decidir qué hacer con lo encontrado. Tus archivos no han cambiado.",
  },
  results: {
    map: {
      label: "Mapa de los elementos evaluados, por tamaño",
      all: "Todos los tipos",
      smaller: (n: number) => `${count(n)} elementos más pequeños`,
      notRead: "No leído",
      notReadYet: "Todavía no se ha leído",
      entries: (n: number) => `${count(n)} ${n === 1 ? "entrada" : "entradas"}`,
    },
    canFree: (size: string) => `Puedes liberar ${size} con la limpieza`,
    nothingTitle: "No hay nada que limpiar",
    nothing:
      "Lumen solo propone elementos permitidos por sus reglas de seguridad, como cachés que la aplicación puede volver a crear.",
    filters: {
      all: "Todos",
    },
    groups: {
      command: "Limpiar con su propio comando",
      developer: "Herramientas de desarrollo",
      app_cache: "Cachés de aplicaciones",
      app_data: "Datos de aplicaciones",
      other: "Otros",
    },
    otherTools: "Otras herramientas",
    commandNote:
      "Solo la herramienta correspondiente debería eliminar estos elementos. Abre uno para ejecutar su comando; antes comprueba qué elimina, porque no se puede deshacer.",
    status: {
      quarantine: "Propuesto",
      command: "Comando propio",
      review: "Requiere revisión",
      keep: "Se conserva",
    },
    columns: {
      name: "Nombre",
      location: "Ubicación",
      status: "Estado",
      size: "Tamaño",
    },
    listHint: "Muévete entre los elementos con las teclas de flecha arriba y abajo.",
    emptyFilter: "No hay elementos con este estado.",
    partial:
      "El análisis se detuvo antes de terminar. Estos son los resultados obtenidos hasta entonces.",
    whenClosed: (size: string, ...apps: string[]) =>
      apps.length === 0
        ? `Cierra las aplicaciones que usan algunos elementos que se conservan y vuelve a analizar para limpiar ${size} más.`
        : `Cierra ${appList(apps)} y vuelve a analizar para limpiar ${size} más.`,
    outOfSpace:
      "El análisis se detuvo porque el disco está casi lleno (Lumen siempre deja 2 GB libres). Estos son los resultados obtenidos hasta entonces. Libera algo de espacio y vuelve a analizar.",
  },
  action: {
    cleanUp: "Limpiar…",
    preparing: "Preparando…",
    nothing: "No hay nada propuesto para limpiar.",
    planFailed: "Lumen no pudo preparar la limpieza. Tus archivos no han cambiado.",
  },
  detail: {
    size: "Tamaño",
    location: "Ubicación",
    decision: "Decisión",
    reasons: "Pruebas",
    aiNoteHint: "Una suposición de la revisión con IA. No cambia la decisión de Lumen.",
    whenClosed: (...apps: string[]) =>
      apps.length === 0
        ? "Cierra las aplicaciones que lo usan y vuelve a analizar para limpiarlo."
        : `Cierra ${appList(apps)} y vuelve a analizar para limpiarlo.`,
    relatedNone: "No hay nada registrado que relacione este elemento con otro.",
    sources: {
      rule: "Regla",
      location: "Ubicación",
      time: "Fecha",
      relation: "Relación",
      process: "Proceso",
      ai: "Revisión con IA",
    },
    owner: (app: string, present: boolean | null) =>
      present === null
        ? `Pertenece a ${app}. Se desconoce si está instalada.`
        : `Pertenece a ${app}, que ${present ? "está instalada" : "no está instalada"}.`,
    inUse: (inUse: boolean): string =>
      inUse ? "Una aplicación lo tiene abierto." : "Ninguna aplicación lo tiene abierto.",
    unseen: (unseen: number | null) =>
      unseen === null
        ? "Ninguna aplicación visible lo tiene abierto."
        : `Ninguna aplicación visible lo tiene abierto; no se pudieron comprobar ${count(unseen)} ${unseen === 1 ? "proceso" : "procesos"}.`,
    changed: (date: string, folder: boolean) =>
      folder
        ? `La entrada de la carpeta cambió por última vez el ${date}; su contenido podría haber cambiado después.`
        : `Último cambio: ${date}.`,
    opened: (date: string, folder: boolean) =>
      `${folder ? "Última lectura de la carpeta" : "Última apertura"}: ${date} o después; es posible que el sistema no registre todos los usos.`,
    notRecorded: (which: "modified" | "accessed"): string =>
      which === "modified"
        ? "No se registró la fecha del último cambio."
        : "No se registró la fecha del último uso.",
    notRead: (which: "modified" | "accessed"): string =>
      which === "modified"
        ? "Se desconoce la fecha del último cambio: no se leyó."
        : "Se desconoce la fecha del último uso: no se leyó.",
    means: {
      keep: "Lumen conserva este elemento y no mueve nada.",
      review: "Lumen te lo deja para que decidas y no lo moverá por su cuenta.",
      command:
        "La herramienta correspondiente debería limpiarlo. Lumen solo ejecuta el comando si se lo pides.",
      quarantine: "Lumen propone moverlo a Cuarentena. Desde allí puedes restaurarlo.",
    },
    lens: {
      tool: (n: number, tool: string, size: string) => `${others(n)} de ${tool} · ${size}`,
      processes: (n: number, size: string, ...apps: string[]) =>
        `${others(n)} también en uso por ${appList(apps)} · ${size}`,
      evidence: (n: number, size: string) =>
        `${others(n)} relacionados según lo registrado en el análisis · ${size}`,
      kind: (n: number, group: string, size: string) => `${others(n)} en ${group} · ${size}`,
    },
  },
  aiNote: {
    likely: (kind: string, confidence: string) => `IA: probablemente ${kind} (${confidence})`,
    unknown: "IA: no se pudo determinar qué es",
    kinds: {
      app_settings: "configuración o datos de una aplicación",
      user_documents: "tus documentos",
      cache: "una caché",
      logs: "registros",
      downloads_or_installers: "descargas o instaladores",
      developer_tool_data: "datos de una herramienta de desarrollo",
      backup_or_archive: "una copia de seguridad o un archivo",
      unknown: "algo que no se pudo identificar",
    },
    confidence: {
      low: "confianza baja",
      medium: "confianza media",
      high: "confianza alta",
    },
  },
  toolCommand: {
    homebrewDownloads:
      "Homebrew limpia las versiones antiguas instaladas y las cachés de descargas recientes. Es posible que una instalación futura tenga que volver a descargar estos archivos.",
    title: (tool: string) => `${tool} puede limpiar esto por su cuenta`,
    copy: "Copiar",
    copied: "Copiado",
    copyFailed: "Lumen no pudo copiarlo. Selecciona el comando y cópialo tú.",
    note: "Lumen no ejecuta este comando. Comprueba qué elimina antes de ejecutarlo.",
    run: "Ejecutar comando…",
    runConfirm: "Ejecutar comando",
    runWarning:
      "Este comando elimina archivos de inmediato. No pasan por Cuarentena, así que Lumen no puede restaurarlos. Comprueba qué elimina antes de ejecutarlo.",
    removesData:
      "Elimina los contenedores detenidos, las redes sin usar, las imágenes sin etiquetas y la caché de compilación sin usar de Docker.",
    running: "En ejecución… puede tardar unos minutos.",
    succeeded: "El comando ha terminado. Vuelve a analizar para ver qué queda.",
    failed: (code: number | null) =>
      code === null
        ? "El comando ha fallado."
        : `El comando ha fallado (código de salida ${code}).`,
    timedOut: "Seguía en ejecución después de 15 minutos, así que Lumen lo detuvo.",
    output: "Mostrar resultado",
    runFailed: "Lumen no pudo ejecutar el comando.",
    runNote: "Ejecutarlo no se puede deshacer. Comprueba qué elimina antes de hacerlo.",
  },
  spaceChange: {
    estimate: (size: string) => `Elementos seleccionados: ${size} estimados.`,
    increased: (size: string) => `El espacio libre observado aumentó en ${size}.`,
    decreased: (size: string) => `El espacio libre observado disminuyó en ${size}.`,
    unchanged: "Las mediciones no muestran cambios en el espacio libre.",
    unknown: "Lumen no pudo medir el cambio en el espacio libre.",
    multiple: "Se midió el espacio libre de cada disco.",
    shared:
      "Los volúmenes pueden compartir espacio libre; por eso las mediciones se muestran por separado.",
    details: "Ver mediciones",
    disk: (index: number) => `Disco ${index}`,
    readings: (before: string, after: string) => `${before} → ${after} disponibles`,
    note: "Otras aplicaciones también afectan a estas mediciones. El espacio en disco puede tardar en recuperarse.",
  },
  coverage: {
    incomplete:
      "Lumen no pudo leer algunas carpetas y no las tocó. El tamaño total podría ser mayor.",
    howTo: "Cómo permitir el acceso total al disco",
  },
  details: {
    title: "Detalles del análisis",
    entries: (entries: string) => `${count(entries)} entradas leídas`,
    files: "Archivos",
    fileCount: (files: string) => count(files),
    size: "Tamaño",
    sizeValue: (logical: string, allocated: string) => `${logical} (${allocated} en disco)`,
    reclaimable: "Espacio que puedes liberar",
    atLeast: (size: string) => `al menos ${size}`,
    complete: "Lumen lo ha leído todo.",
    incomplete: (denied: string, failed: string, notScanned: string) =>
      `Lumen no pudo leerlo todo: acceso denegado a ${count(denied)}, errores en ${count(failed)}` +
      ` y ${count(notScanned)} sin analizar.`,
  },
  confirm: {
    title: (n: number, size: string) => `¿Mover ${items(n)} (${size}) a Cuarentena?`,
    checked:
      "Lumen vuelve a comprobar cada elemento antes de moverlo y deja en su sitio cualquiera que haya cambiado desde el análisis.",
    restorable:
      "Puedes restaurarlo todo desde Cuarentena durante al menos 30 días, hasta que lo elimines definitivamente.",
    nothingLeft: "Ya no queda nada que mover. Vuelve a analizar para ver qué puedes limpiar.",
    items: "Elementos que se moverán",
    move: "Mover a Cuarentena",
    moving: "Moviendo…",
    failed: "Lumen no pudo realizar la limpieza.",
    planMade: (createdAt: string) =>
      `Lumen creó este plan ${createdAt}. Puedes usarlo durante 24 horas.`,
    planHash: "Hash del plan",
  },
  done: {
    title: (n: number, size: string) => `${items(n)} (${size}) se movieron a Cuarentena.`,
    noneMoved: "Lumen no movió nada.",
    freedNote: "Recuperarás el espacio cuando los elimines definitivamente de Cuarentena.",
    openQuarantine: "Abrir Cuarentena",
    notMoved: (n: number) => `${items(n)} no se movieron`,
  },
  item: {
    quarantined: "Se movió a Cuarentena. Puedes restaurarlo.",
    skipped: (why: string) => `No se movió: ${why}.`,
    failed: (reason: string, where: string) => `No se pudo mover: ${reason}; ${where}.`,
    failIdentityMismatch: "cambió después del análisis",
    failInUse: "un programa lo está usando",
    failVolumeUnsupported: "esta unidad no puede alojar una cuarentena",
    failMoveFailed: "se produjo un error al moverlo",
    failVerificationFailed: "después de moverlo no coincidía con el plan",
    failOther: "por un motivo que esta versión desconoce",
    skipChanged: "cambió o ya no se puede comprobar",
    skipOutsideRoots: "está fuera de las carpetas analizadas",
    skipNowKeep: "Lumen ahora lo conserva",
    skipNowReview: "Lumen ahora quiere que lo revises",
    skipReasonsChanged: "ahora se propondría por otros motivos",
    skipNotPrepared: "Lumen no pudo registrarlo para moverlo de forma segura",
    skipManifest: "Lumen no pudo escribir su registro",
    leftSource: "permanece intacto en su lugar",
    leftQuarantine: "está en Cuarentena y se puede restaurar",
    leftGone: "ya no está",
  },
  quarantine: {
    title: "Cuarentena",
    atLeast: (size: string) => `al menos ${size}`,
    columns: {
      name: "Nombre",
      location: "Ubicación original",
      size: "Tamaño",
      held: "Tiempo en Cuarentena",
    },
    loadFailed: "Lumen no pudo cargar Cuarentena.",
    empty: "Cuarentena está vacía",
    emptyHint:
      "Los elementos que limpias se guardan aquí para que puedas restaurarlos si cambias de opinión.",
    goCleanUp: "Abrir Explorar",
    interrupted: (n: number) =>
      `La limpieza se detuvo a medias y Lumen no puede determinar dónde está ${n === 1 ? "este elemento" : `estos ${count(n)} elementos`}. Lumen no ${n === 1 ? "lo eliminó" : "los eliminó"}. Comprueba dónde ${n === 1 ? "estaba" : "estaban"}:`,
    selection: {
      all: "Seleccionar todo",
      item: (name: string) => `Seleccionar ${name}`,
      count: (n: number, size: string) => `${items(n)} seleccionados · ${size}`,
      clear: "Quitar selección",
      restore: "Restaurar",
      removeAsk: "Eliminar definitivamente…",
      removeWarning: (n: number) =>
        `Esto eliminará ${items(n)} definitivamente y no se podrá deshacer. Primero, Lumen comprueba que nada esté usando cada elemento.`,
      removeSkips: (n: number) =>
        `${items(n)} que solo se pueden restaurar ${n === 1 ? "permanece" : "permanecen"} en Cuarentena.`,
      remove: "Eliminar definitivamente",
      working: (done: number, total: number) => `Procesando… ${count(done)} de ${count(total)}`,
      restored: (well: number, total: number) =>
        well === total
          ? `${items(total)} restaurados.`
          : `Se restauraron ${count(well)} de ${items(total)}. El resto sigue en Cuarentena.`,
      removed: (well: number, total: number) =>
        well === total
          ? `${items(total)} eliminados definitivamente.`
          : `Se eliminaron definitivamente ${count(well)} de ${items(total)}. Lumen no eliminó nada más.`,
      restoredStopped: (well: number, total: number, untried: number) =>
        [
          `Se restauraron ${count(well)} de ${items(total)} y después un error detuvo a Lumen.`,
          untried > 0 ? `No se intentó restaurar los otros ${count(untried)}.` : null,
          "Lo que sigue en la lista permanece en Cuarentena.",
        ]
          .filter((sentence) => sentence !== null)
          .join(" "),
      unverified: (n: number) =>
        n === 1
          ? "1 elemento restaurado no coincide exactamente con su registro; compruébalo."
          : `${count(n)} elementos restaurados no coinciden exactamente con su registro; compruébalos.`,
      removedStopped: (well: number, total: number, untried: number) =>
        [
          `Se eliminaron definitivamente ${count(well)} de ${items(total)} y después un error detuvo a Lumen.`,
          untried > 0 ? `No se intentó eliminar los otros ${count(untried)}.` : null,
          "Lo que sigue en la lista permanece en Cuarentena y se puede restaurar.",
        ]
          .filter((sentence) => sentence !== null)
          .join(" "),
    },
  },
  held: {
    unknownPath: "(ubicación desconocida)",
    days: (days: number) => (days === 0 ? "Hoy" : `${count(days)} ${days === 1 ? "día" : "días"}`),
    restoreOnly: "Solo restaurar",
    afterFailure:
      "Se produjo un error al moverlo, así que Lumen lo guardó aquí de forma segura. Solo puedes restaurarlo.",
    restore: "Restaurar",
    removeForGoodAsk: "Eliminar definitivamente…",
    removeForGoodWarning:
      "Esto lo eliminará definitivamente y no se podrá deshacer. Primero, Lumen comprueba que nada lo esté usando.",
    removeForGood: "Eliminar definitivamente",
    failed: "Lumen no pudo hacerlo. Vuelve a intentarlo.",
  },
  restore: {
    restored: "Se restauró en su ubicación original.",
    parentReplaced: "La carpeta se había vuelto a crear entretanto.",
    unverified: "No coincide exactamente con su registro; compruébalo.",
    notRestored: (why: string) => `Lumen no pudo restaurarlo. Sigue en Cuarentena. ${why}.`,
    nameTaken:
      "Ahora hay algo con ese nombre en la ubicación original. Retíralo e inténtalo de nuevo",
    originUnreachable: "Lumen no puede acceder de forma segura a la carpeta original",
    itemChanged: "Cambió mientras estaba en Cuarentena",
    moveFailed: "No se pudo mover de vuelta",
    notRestorable: "Lumen no puede restaurar este elemento en su estado actual.",
  },
  finalize: {
    finalized: "Eliminado definitivamente.",
    foundAtSource: "Ya está en su ubicación original. Lumen no eliminó nada.",
    notFound:
      "Lumen no pudo encontrarlo en Cuarentena. Si estaba guardado en otra unidad, conéctala e inténtalo de nuevo. Lumen no eliminó nada.",
    failed: (what: string) => `Lumen no pudo eliminarlo por completo. ${what}.`,
    removedNothing: "No se eliminó nada",
    removedPartly: "Se eliminó una parte. Vuelve a intentarlo para terminar",
    removedItem: "El elemento ya no está; solo queda su registro",
    maybeInUse:
      "Es posible que esté en uso, o que Lumen no pueda comprobarlo, así que no eliminó nada. Cierra las aplicaciones que lo usan e inténtalo de nuevo.",
    notFinalizable: "Este elemento solo se puede restaurar.",
  },
  settings: {
    title: "Ajustes",
    appearance: {
      title: "Tema",
      system: "Sistema",
      light: "Claro",
      dark: "Oscuro",
      failed: "Lumen no pudo cambiar la apariencia de la ventana.",
    },
    permissions: {
      title: "Permisos",
      fda: "Acceso total al disco",
      fdaIntro:
        "Puedes comprobar el acceso en Ajustes del Sistema de macOS. Si ya permitiste el acceso a Lumen, puedes omitir esta guía. Es posible que algunas carpetas protegidas sigan siendo ilegibles incluso con el permiso.",
      steps: "Mostrar los pasos de configuración",
      step1: "Abre Ajustes del Sistema.",
      step2: "Ve a Privacidad y seguridad › Acceso total al disco.",
      step3: "Activa Lumen. Si ya está activado, déjalo así.",
      step4:
        "Si cambiaste el ajuste, cierra Lumen por completo, vuelve a abrirlo e inicia un análisis nuevo.",
      open: "Abrir Ajustes del Sistema",
      openFailed: "Lumen no pudo abrir Ajustes del Sistema. Ábrelos desde el menú Apple.",
      access: "Acceso a las carpetas",
      accessIntro:
        "Es posible que el ordenador impida a Lumen acceder a algunas carpetas. Lumen sigue leyendo todas las carpetas permitidas.",
      unread:
        "Lumen muestra como ilegible cualquier carpeta que no puede leer, nunca como vacía, y no propone nada de su contenido.",
    },
    about: {
      title: "Acerca de",
      version: "Versión",
      local: "En este dispositivo",
      localBody:
        "Los análisis, las decisiones y Cuarentena permanecen en este ordenador. El contenido de los archivos nunca se envía a ningún lugar. Mientras la revisión con IA está activada, solo se envían detalles como rutas y tamaños.",
      update: {
        title: "Actualizaciones",
        checking: "Buscando una nueva versión…",
        upToDate: "Lumen está actualizado.",
        available: (version: string) => `La versión ${version} está disponible.`,
        packageOnly:
          "Esta copia se instaló desde un paquete: consigue la nueva versión en zaewc.github.io/Lumen.",
        checkFailed: "Lumen no pudo buscar una nueva versión.",
        check: "Buscar ahora",
        install: "Actualizar y reiniciar",
        installing: "Actualizando…",
        installingNote:
          "Lumen descarga la nueva versión, comprueba su firma, la instala y se reinicia.",
        installFailed: "No se instaló la actualización. Esta versión no ha cambiado.",
        waitForItems: "Puedes actualizar cuando Lumen termine de mover o eliminar elementos.",
        sidebar: (version: string) => `Versión ${version} disponible`,
        notice: (version: string) => `Lumen ${version} está disponible.`,
        noticeAction: "Ver actualización",
        later: "Más tarde",
      },
    },
  },
  whatIs: (term: string) => `¿Qué significa «${term}»?`,
  glossary: {
    verdicts: {
      title: "Cómo decide Lumen",
      body: "Lumen decide qué hacer con cada elemento mediante reglas de seguridad fijas, no mediante suposiciones. «Propuesto»: elementos como cachés que su aplicación o herramienta puede volver a crear. «Limpiar con su propio comando»: elementos que solo debería eliminar su herramienta; Lumen muestra el comando para que lo ejecutes tú. «Requiere revisión»: elementos grandes sobre los que Lumen no tiene certeza o restos de aplicaciones desinstaladas; nunca se mueven automáticamente. «Se conserva»: elementos que Lumen no tocará, como archivos del sistema, archivos en uso, datos propios de aplicaciones o elementos pequeños que no reconoce. Si hay dudas, Lumen deja el elemento donde está.",
    },
    quarantine: {
      title: "Cuarentena",
      body: "Cuarentena es un lugar seguro donde guardar elementos. Al limpiar, se mueven allí en vez de eliminarse, y puedes restaurarlos en su ubicación original hasta que los elimines definitivamente. Hasta entonces, los elementos en Cuarentena siguen ocupando espacio en disco.",
    },
  },
  reviewReasons: {
    unknownOwner: "No se ha identificado qué aplicación o herramienta lo creó.",
    sharedResource: "Comparte espacio con otros archivos o tiene enlaces fuera de este elemento.",
    staleEvidence: "Parte de la evidencia está desactualizada. Vuelve a analizar antes de decidir.",
    uncertainSize: "No se sabe con certeza cuánto espacio liberaría su eliminación definitiva.",
  },
  reasons: {
    location: (name: string) => `Se conserva: está en una ubicación protegida (${name}).`,
    rules: {
      "default.review": "Requiere revisión: Lumen no reconoce este elemento.",
      "default.keep-small":
        "Se conserva: es demasiado pequeño como para que valga la pena revisarlo.",
      "protect.os-protected": "Se conserva: el sistema operativo protege este elemento.",
      "protect.system-file": "Se conserva: Windows marca este elemento como archivo del sistema.",
      "protect.cloud-backed":
        "Se conserva: está sincronizado con un servicio en la nube, así que quitarlo aquí podría eliminarlo en todas partes.",
      "protect.in-use": "Se conserva: una aplicación en ejecución lo está usando.",
      "protect.in-use-partial":
        "Se conserva: ninguna aplicación visible para Lumen lo usa, pero Lumen no pudo comprobar todas las aplicaciones.",
      "protect.in-use-unknown": "Se conserva: Lumen no ha comprobado si algo lo está usando.",
      "protect.not-observed": "Se conserva: el análisis no ha detectado este elemento.",
      "protect.incomplete-scan":
        "Se conserva: Lumen no pudo leer una parte, así que se desconoce su contenido.",
      "protect.location-data":
        "Se conserva: Lumen no pudo comprobar su lista de ubicaciones protegidas.",
      "protect.location-context":
        "Se conserva: Lumen no pudo comprobar su lista de ubicaciones protegidas.",
      "protect.contains-system": "Se conserva: contiene una parte del sistema operativo.",
      "protect.contains-sensitive":
        "Se conserva: contiene contraseñas, claves o sesiones de un navegador web, así que Lumen no lo toca.",
      "protect.contains-cloud":
        "Se conserva: contiene una carpeta sincronizada con la nube, así que quitarlo aquí podría eliminar esos archivos en todas partes.",
      "eligible.quarantine-unsupported":
        "Requiere revisión: este dispositivo no puede moverlo a Cuarentena de forma reversible.",
      "eligible.frees-nothing": "Se conserva: eliminarlo no liberaría espacio.",
      "eligible.object-kind": "Requiere revisión: es un enlace o un archivo especial, no datos.",
      "eligible.in-use-partial":
        "Ninguna aplicación visible para Lumen lo usa. Lumen no pudo comprobar las aplicaciones del sistema, así que volverá a comprobarlo justo antes de moverlo.",
      "eligible.in-use-partial-single-user":
        "Ninguna aplicación visible para Lumen lo usa. Eres la única persona que usa este ordenador, así que lo que Lumen no pudo comprobar corresponde al sistema. Volverá a comprobarlo justo antes de moverlo.",
      "classify.devkb-regenerable": "Propuesto: la herramienta que lo creó puede volver a crearlo.",
      "classify.devkb-tool-action":
        "Requiere revisión: solo debería eliminarlo el comando de limpieza de la propia herramienta, y no se puede deshacer.",
      "classify.devkb-not-regenerable": "Requiere revisión: nada puede volver a crearlo.",
      "classify.devkb-self-managed":
        "Se conserva: la herramienta que lo creó lo limpia automáticamente.",
      "classify.devkb-unknown":
        "Requiere revisión: Lumen no pudo cargar la información correspondiente a esta herramienta.",
      "classify.vendor-declared-cache":
        "Requiere revisión: quien lo creó indica que se puede eliminar.",
      "classify.browser-profile":
        "Se conserva: contiene sesiones, cookies o datos de sitios web de un navegador, así que Lumen no lo toca.",
      "classify.app-cache":
        "Propuesto: es una caché de una aplicación que esta puede volver a crear.",
      "classify.app-state":
        "Se conserva: contiene datos, ajustes o documentos propios de una aplicación, así que Lumen no lo toca.",
      "classify.orphan":
        "Requiere revisión: parece que la aplicación a la que pertenece está desinstalada.",
      "jev.demote": "Requiere revisión: la revisión con IA recomendó actuar con cautela.",
    },
  },
};
