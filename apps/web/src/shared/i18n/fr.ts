import { en, type Messages } from "./en";
import { countFormat } from "./format";

const count = countFormat("fr");
const items = (n: number) => `${count(n)} ${n > 1 ? "éléments" : "élément"}`;
const others = (n: number) => `${count(n)} ${n > 1 ? "autres éléments" : "autre élément"}`;

function appList(apps: readonly string[]): string {
  if (apps.length > 3) {
    return `${apps.slice(0, 3).join(", ")} et ${count(apps.length - 3)} de plus`;
  }
  return new Intl.ListFormat("fr", { type: "conjunction" }).format(apps);
}

/** Catalogue français de l'interface. */
export const fr: Messages = {
  ...en,
  language: "Langue",
  nav: {
    label: "Menu principal",
    overview: "Stockage",
    cleanUp: "Explorer",
    quarantine: "Quarantaine",
    apps: "Apps",
    settings: "Paramètres",
    held: (n) => `${items(n)} en quarantaine`,
  },
  common: {
    loading: "Chargement…",
    close: "Fermer",
    details: "Détails",
    noDetails: "Lumen n'a pas obtenu de détails sur cette erreur.",
    tryAgain: "Réessayer",
    scanDisk: "Analyser le disque",
    scanning: "Analyse en cours…",
    scanAgain: "Analyser à nouveau",
    scanOther: "Analyser d'autres dossiers",
    items: (n) => items(n),
  },
  overview: {
    ...en.overview,
    title: "Stockage",
    disk: {
      title: "Disque de démarrage",
      meter: "Utilisation du disque",
      used: (size) => `${size} utilisés`,
      total: (size) => `${size} au total`,
      percentUsed: (percent) => `${percent} % utilisés`,
      assessed: (size) => `Évalué : ${size}`,
      low: "Presque plein : moins de 10 % d'espace disponible.",
      loadFailed: "Lumen n'a pas pu lire l'utilisation du disque.",
    },
    session: {
      title: "Dernière analyse",
      quarantine: "En quarantaine",
      atLeast: (size) => `au moins ${size}`,
      found: (n) => `${n} lus`,
      openExplore: "Ouvrir Explorer",
      showProgress: "Afficher la progression",
      cleaned: (n, size) => `${items(n)} (${size}) déplacés en quarantaine.`,
      none: "Aucune analyse pour le moment. Analysez votre disque pour explorer son contenu et les éléments qui justifient chaque décision.",
    },
    tools: {
      title: "Outils de développement",
      intro:
        "Demandez à Xcode, Docker et aux gestionnaires de paquets combien d'espace ils occupent. Lumen ne fait que lire leurs réponses.",
      check: "Vérifier les outils de développement",
      checking: "Vérification…",
      checkAgain: "Vérifier à nouveau",
      loadFailed: "Lumen n'a pas pu interroger les outils de développement.",
      notInstalled: "Non installé",
      unsupported: "Non disponible sur cet ordinateur",
      unavailable: "Vérification impossible",
      none: "Aucun élément signalé",
      inUse: (detail) => `${detail} utilisé`,
      reclaimable: (size) => `Docker considère ${size} comme inutilisé`,
      cache: "Cache",
      sharedStore: "Stockage partagé",
      ownCommand: "Commande dédiée",
      xcodeManage: "Gérez les environnements d'exécution dans Xcode › Réglages › Composants.",
      dockerManage:
        "Le décompte des éléments inutilisés de Docker inclut les volumes et les conteneurs arrêtés, qui peuvent contenir des données utiles. Son nettoyage ne se lance que depuis Explorer, après confirmation de la commande complète, et conserve les volumes.",
      sharedStoreNote:
        "Les projets sont liés à ce stockage. Sa commande dédiée conserve les éléments encore utilisés.",
      cacheNote: "Téléchargé à nouveau si nécessaire.",
    },
    copies: {
      title: "Copies identiques",
      intro:
        "Une fois l'analyse terminée, Lumen lit séparément les fichiers des dossiers analysés pour repérer les contenus identiques. Cette comparaison est en lecture seule : Lumen ne choisit ni ne supprime aucune copie.",
      compareAgain: "Comparer à nouveau",
      stop: "Arrêter",
      stopping: "Arrêt en cours…",
      stopped: "Comparaison arrêtée. Aucun résultat de cette comparaison n'est affiché.",
      comparing: "Comparaison…",
      stage: {
        head: "Comparaison du début des fichiers de même taille",
        tail: "Comparaison de la fin des fichiers encore identiques",
        full: "Comparaison des fichiers entiers encore identiques",
      },
      filesRead: (n) => `${count(n)} fichier${n === "1" ? "" : "s"} lu${n === "1" ? "" : "s"}`,
      folders: "Dossiers de l'analyse comparés",
      group: (n, size) => `${count(n)} copies · ${size} chacune`,
      none: "Aucune copie identique parmi les fichiers que Lumen a pu comparer.",
      more: (n) =>
        `${count(n)} groupe${n > 1 ? "s" : ""} plus petit${n > 1 ? "s" : ""} non affiché${n > 1 ? "s" : ""}.`,
      hardLinks: (n) =>
        n === 1
          ? "1 fichier porte plusieurs noms (liens physiques). Il s'agit d'un seul fichier, pas de copies."
          : `${count(n)} fichiers portent plusieurs noms (liens physiques). Chacun reste un seul fichier, pas des copies.`,
      skipped: (n) =>
        `${count(n)} fichier${n === 1 ? " n'a pas" : "s n'ont pas"} été comparé${n === 1 ? "" : "s"} : modifié${n === 1 ? "" : "s"}, illisible${n === 1 ? "" : "s"}, uniquement dans le cloud ou protégé${n === 1 ? "" : "s"} par le système.`,
      protectedFiles: (n) =>
        `${count(n)} fichier${n === 1 ? "" : "s"} dans des emplacements protégés laissé${n === 1 ? "" : "s"} sans lecture.`,
      incomplete:
        "Certains dossiers n'ont pas pu être lus entièrement. Leurs fichiers n'ont pas été comparés.",
      unsupported: "La comparaison de fichiers n'est pas encore disponible sur cet ordinateur.",
      tooMany: (limit) =>
        `L'analyse couvre plus de ${count(limit)} fichiers. Relancez une analyse plus ciblée pour en comparer moins.`,
      failed: "Lumen n'a pas pu comparer les fichiers de l'analyse.",
      advice:
        "Certaines copies peuvent être des sauvegardes à conserver. Lumen ne choisit ni ne supprime aucune copie. La suppression d'une copie peut libérer moins d'espace que sa taille.",
    },
  },
  workspace: {
    available: "Espace disponible",
    assessed: "Éléments évalués · par taille",
    assessment: (n) => `${items(n)} évalués`,
    unknownOrigin: "Origine inconnue",
    workflow: "De l'analyse au nettoyage",
    scanStage: "Analyser",
    resultsStage: "Voir les résultats",
    cleanUpStage: "Nettoyer",
    activeCommand: "Commande en cours",
    quitHeld:
      "Lumen déplace ou supprime des éléments. Vous pourrez le fermer une fois l'opération terminée.",
  },
  start: {
    title: "Découvrez le contenu de votre disque",
    promise:
      "L'analyse ne fait que lire et dure quelques minutes. Lumen vous demande votre accord avant tout déplacement.",
    wholeDiskHint: "Votre ordinateur peut vous demander d'autoriser l'accès à certains dossiers.",
    chooseFolders: "Analyser seulement certains dossiers",
    foldersLabel: "Dossiers à analyser, un par ligne",
    foldersEmpty: "Saisissez au moins un dossier.",
    scanFolders: "Analyser ces dossiers",
    failed: "L'analyse n'a pas pu aboutir. Vos fichiers n'ont pas été modifiés.",
    aiOn: (service) =>
      `L'analyse par IA est activée : Lumen envoie au serveur ${service} les détails des éléments proposés et de certains éléments à examiner, jamais leur contenu. Vous pouvez désactiver cette option dans les paramètres.`,
  },
  ai: {
    ...en.ai,
    title: "Analyse par IA",
    offIntro:
      "Un modèle d'IA examine les éléments que Lumen propose de nettoyer et indique ce que pourraient être ceux à examiner. Il ne peut que rendre Lumen plus prudent.",
    onIntro: (key) =>
      `Activée avec ${key}. Lumen envoie les détails de chaque élément proposé et de jusqu'à 20 éléments à examiner, jamais leur contenu. L'IA ne peut que rendre Lumen plus prudent. Vous pouvez la désactiver à tout moment. Elle envoie aussi le nom, l'éditeur et la version des apps installées, sans chemin.`,
    keyInUse: {
      builtin: (service) => `${service} (clé intégrée)`,
      stored: (service) => `${service} (votre clé)`,
    },
    checking: "Vérification…",
    short: { on: "Analyse par IA activée", off: "Analyse par IA désactivée" },
    loadFailed: "Lumen n'a pas pu charger le réglage de l'analyse par IA.",
    changeFailed: "Lumen n'a pas pu modifier le réglage de l'analyse par IA.",
    consent: {
      title: "Activer l'analyse par IA ?",
      what: "Pour chaque élément proposé et jusqu'à 20 des éléments à examiner les plus volumineux, Lumen envoie son chemin (votre dossier personnel apparaît sous la forme ~), sa taille, son ancienneté et les éléments qui justifient la décision. Le contenu des fichiers n'est jamais envoyé. Pour la page Apps, Lumen envoie aussi le nom, l'éditeur et la version des apps installées et les éléments d'arrière-plan qui leur sont liés, sans chemin.",
      whereJeb:
        "Lumen envoie ces informations avec votre clé API au serveur Jeb de votre établissement (jeb.https.gsmsv.site). Son exploitant peut conserver les requêtes.",
      whereJebBuiltin:
        "Lumen envoie ces informations avec sa clé intégrée au serveur Jeb d'un établissement (jeb.https.gsmsv.site). Son exploitant peut conserver les requêtes.",
      whereOpenRouter:
        "Lumen envoie ces informations avec votre clé API à Jev de TypeSafe via OpenRouter.",
      cautious:
        "L'IA ne peut que rendre une décision plus prudente. Pour les éléments à examiner, elle ajoute seulement une indication sur leur nature probable. Elle ne choisit jamais ce qui est déplacé ou supprimé.",
      revocable:
        "L'analyse par IA s'exécutera à chaque analyse. Vous pouvez la désactiver dans les paramètres à tout moment.",
      keyLabel: "Clé API Jeb",
      keyHint: "Lumen la conserve dans le stockage sécurisé de votre système et ne l'affiche plus.",
      keyInvalid: "Collez la clé entière : 1 à 256 caractères, sans espace.",
      ownKey: "Utiliser votre propre clé",
      ownKeyHint:
        "Facultatif. Lumen l'utilise à la place de la clé intégrée, la conserve dans le stockage sécurisé de votre système et ne l'affiche plus.",
      confirm: "Activer l'analyse par IA",
      turningOn: "Activation…",
    },
    firstLaunch: {
      title: "Utiliser l'analyse par IA ?",
      intro:
        "Lumen prend ses décisions selon ses propres règles de sécurité. Si vous le souhaitez, un modèle d'IA peut les examiner. Aucune donnée n'est envoyée avant votre accord.",
      on: "Activer l'analyse par IA",
      off: "Continuer sans l'analyse par IA",
    },
  },
  scanning: {
    title: "Analyse en cours…",
    deciding: "Décision en cours…",
    decidingHint:
      "Lumen détermine la nature de chaque élément et ce qu'il convient d'en faire. Rien n'est modifié.",
    stages: {
      inventory: "Vérification des applications installées…",
      markers: "Recherche de l'origine des éléments…",
      evidence: "Repérage des caches et dossiers d'outils…",
      graph: "Association des éléments à leurs applications et outils…",
      deciding: "Application des règles de sécurité…",
      describing: "L'analyse par IA décrit les éléments à examiner…",
    },
    stagePercent: (n) => `${n} % de cette étape`,
    processed: (done, total) =>
      `${done} élément${done === "1" ? "" : "s"} vérifié${done === "1" ? "" : "s"} sur ${total}`,
    readOnly: "Lumen ne fait que lire et ne modifie rien.",
    progress: "Progression de l'analyse",
    items: "Entrées lues",
    unreadable: "Dossiers illisibles",
    reading: "Lecture",
    readSoFar: (size) => `Déjà lu : ${size}`,
    failed:
      "Lumen n'a pas pu décider quoi faire des éléments trouvés. Vos fichiers n'ont pas été modifiés.",
  },
  results: {
    ...en.results,
    map: {
      label: "Carte des éléments évalués, par taille",
      all: "Toutes les catégories",
      smaller: (n) => `${count(n)} éléments plus petits`,
      notRead: "Non lu",
      notReadYet: "Pas encore lu",
      entries: (n) => `${count(n)} entrée${n === 1 ? "" : "s"}`,
    },
    canFree: (size) => `Vous pouvez libérer ${size}`,
    nothingTitle: "Rien à nettoyer",
    nothing:
      "Lumen ne propose que les éléments autorisés par ses règles de sécurité, comme les caches que leur application peut recréer.",
    filters: { all: "Tout" },
    groups: {
      command: "Nettoyer avec la commande dédiée",
      developer: "Outils de développement",
      app_cache: "Caches d'applications",
      app_data: "Données d'applications",
      other: "Autre",
    },
    otherTools: "Autres outils",
    commandNote:
      "Seul l'outil concerné devrait supprimer ces éléments. Ouvrez-le pour exécuter sa commande et vérifiez d'abord ce qu'elle supprime : cette action est irréversible.",
    status: {
      quarantine: "Proposé",
      command: "Commande dédiée",
      review: "À examiner",
      keep: "Conservé",
    },
    columns: { name: "Nom", location: "Emplacement", status: "Statut", size: "Taille" },
    listHint: "Parcourez les éléments avec les flèches haut et bas.",
    emptyFilter: "Aucun élément avec ce statut.",
    partial: "L'analyse s'est arrêtée avant la fin. Voici ce qu'elle a trouvé jusque-là.",
    whenClosed: (size, ...apps) =>
      apps.length === 0
        ? `Quittez les applications qui utilisent certains éléments conservés, puis relancez l'analyse pour nettoyer ${size} de plus.`
        : `Quittez ${appList(apps)}, puis relancez l'analyse pour nettoyer ${size} de plus.`,
    outOfSpace:
      "L'analyse s'est arrêtée, car le disque est presque plein (Lumen conserve toujours 2 Go d'espace libre). Voici les résultats obtenus jusque-là. Libérez de l'espace et relancez l'analyse.",
  },
  action: {
    cleanUp: "Nettoyer…",
    preparing: "Préparation…",
    nothing: "Aucun élément n'est proposé au nettoyage.",
    planFailed: "Lumen n'a pas pu préparer le nettoyage. Vos fichiers n'ont pas été modifiés.",
  },
  detail: {
    ...en.detail,
    size: "Taille",
    location: "Emplacement",
    decision: "Décision",
    reasons: "Éléments justificatifs",
    aiNoteHint: "Une estimation de l'analyse par IA. Elle ne modifie pas la décision de Lumen.",
    whenClosed: (...apps) =>
      apps.length === 0
        ? "Quittez les applications qui l'utilisent, puis relancez l'analyse pour le nettoyer."
        : `Quittez ${appList(apps)}, puis relancez l'analyse pour le nettoyer.`,
    relatedNone: "Aucun lien avec un autre élément n'a été enregistré.",
    sources: {
      rule: "Règle",
      location: "Emplacement",
      time: "Date",
      relation: "Relation",
      process: "Processus",
      ai: "Analyse par IA",
    },
    owner: (app, present) =>
      present === null
        ? `Appartient à ${app}. On ignore si cette application est installée.`
        : `Appartient à ${app}, qui est ${present ? "installée" : "désinstallée"}.`,
    inUse: (inUse) => (inUse ? "Une application l'a ouvert." : "Aucune application ne l'a ouvert."),
    unseen: (unseen) =>
      unseen === null
        ? "Aucune application visible ne l'a ouvert."
        : `Aucune application visible ne l'a ouvert ; ${count(unseen)} processus n'ont pas pu être vérifiés.`,
    changed: (date, folder) =>
      folder
        ? `L'entrée du dossier a été modifiée pour la dernière fois le ${date} ; son contenu a pu changer depuis.`
        : `Dernière modification : ${date}.`,
    opened: (date, folder) =>
      `${folder ? "Dernière lecture du dossier" : "Dernière ouverture"} : ${date} ou après ; le système n'enregistre peut-être pas chaque utilisation.`,
    notRecorded: (which) =>
      which === "modified"
        ? "Aucune date de dernière modification n'a été enregistrée."
        : "Aucune date de dernière utilisation n'a été enregistrée.",
    notRead: (which) =>
      which === "modified"
        ? "Date de dernière modification inconnue : non lue."
        : "Date de dernière utilisation inconnue : non lue.",
    means: {
      keep: "Lumen conserve cet élément et ne le déplace pas.",
      review: "Lumen vous laisse en juger et ne le déplacera pas de lui-même.",
      command:
        "Son propre outil devrait le nettoyer. Lumen n'exécute la commande que si vous le demandez.",
      quarantine:
        "Lumen propose de déplacer cet élément en quarantaine. Vous pourrez l'en restaurer.",
    },
    recheck: {
      unknown: "Au moment de la décision, Lumen ignorait :",
      gaps: {
        in_use: "si une application l'utilisait",
        unread: "ce qu'il contenait",
        stale: "son état actuel",
        owner: "à quelle application il appartient et si elle est installée",
      },
      action: "Vérifier à nouveau",
      checking: "Vérification…",
      note: "La vérification ne fait que lire. Rien n'est déplacé.",
      settled:
        "Aucune vérification nécessaire : la décision ne dépend d'aucune information manquante.",
      changed:
        "L'élément a changé depuis l'analyse ; Lumen n'a donc pas repris sa décision. Relancez l'analyse.",
      still: (status) => `Nouvelle vérification · toujours ${status}`,
      now: (before, after) => `Nouvelle vérification · ${before} → ${after}`,
      failed: "Lumen n'a pas pu vérifier à nouveau cet élément.",
    },
    lens: {
      tool: (n, tool, size) => `${others(n)} de ${tool} · ${size}`,
      processes: (n, size, ...apps) =>
        `${others(n)} également utilisés par ${appList(apps)} · ${size}`,
      evidence: (n, size) =>
        `${others(n)} liés par les éléments enregistrés lors de l'analyse · ${size}`,
      kind: (n, group, size) => `${others(n)} dans ${group} · ${size}`,
    },
  },
  aiNote: {
    likely: (kind, confidence) => `IA : probablement ${kind} (${confidence})`,
    unknown: "IA : nature indéterminée",
    kinds: {
      app_settings: "des paramètres ou données d'application",
      user_documents: "vos documents",
      cache: "un cache",
      logs: "des journaux",
      downloads_or_installers: "des téléchargements ou installateurs",
      developer_tool_data: "des données d'un outil de développement",
      backup_or_archive: "une sauvegarde ou une archive",
      unknown: "un élément indéterminé",
    },
    confidence: {
      low: "faible confiance",
      medium: "confiance moyenne",
      high: "forte confiance",
    },
  },
  toolCommand: {
    homebrewDownloads:
      "Homebrew supprime les anciennes versions installées et les caches de téléchargement récents. Une prochaine installation pourrait devoir télécharger ces fichiers à nouveau.",
    title: (tool) => `${tool} peut nettoyer cet élément lui-même`,
    copy: "Copier",
    copied: "Copié",
    copyFailed: "Lumen n'a pas pu le copier. Sélectionnez la commande et copiez-la vous-même.",
    note: "Lumen n'exécute pas cette commande. Vérifiez ce qu'elle supprime avant de la lancer.",
    run: "Exécuter la commande…",
    runConfirm: "Exécuter la commande",
    runWarning:
      "Cette commande supprime immédiatement des fichiers. Ils ne passent pas par la quarantaine et Lumen ne peut pas les restaurer. Vérifiez ce qu'elle supprime avant de la lancer.",
    removesData:
      "Elle supprime les conteneurs Docker arrêtés, les réseaux inutilisés, les images orphelines et le cache de compilation inutilisé.",
    running: "Exécution… cela peut prendre quelques minutes.",
    succeeded: "La commande est terminée. Relancez l'analyse pour voir ce qu'il reste.",
    failed: (code) =>
      code === null ? "La commande a échoué." : `La commande a échoué (code ${code}).`,
    timedOut: "La commande tournait encore après 15 minutes ; Lumen l'a arrêtée.",
    output: "Afficher la sortie",
    runFailed: "Lumen n'a pas pu exécuter la commande.",
    runNote: "Cette action est irréversible. Vérifiez ce qu'elle supprime avant de la lancer.",
  },
  spaceChange: {
    estimate: (size) => `Éléments sélectionnés : ${size} estimés.`,
    increased: (size) => `Augmentation constatée de l'espace libre : ${size}.`,
    decreased: (size) => `Diminution constatée de l'espace libre : ${size}.`,
    unchanged: "Les mesures ne montrent aucun changement de l'espace libre.",
    unknown: "Lumen n'a pas pu mesurer le changement d'espace libre.",
    multiple: "L'espace libre a été mesuré pour chaque disque.",
    shared:
      "Les volumes peuvent partager l'espace libre ; ces mesures sont donc affichées séparément.",
    details: "Voir les mesures",
    disk: (index) => `Disque ${index}`,
    readings: (before, after) => `${before} → ${after} disponibles`,
    note: "D'autres applications influent aussi sur ces mesures. La récupération de l'espace peut prendre du temps.",
  },
  coverage: {
    incomplete:
      "Lumen n'a pas pu lire certains dossiers et les a laissés intacts. Les tailles peuvent être supérieures.",
    howTo: "Comment autoriser l'accès complet au disque",
  },
  details: {
    title: "Détails de l'analyse",
    entries: (entries) => `${count(entries)} entrées lues`,
    files: "Fichiers",
    fileCount: (files) => count(files),
    size: "Taille",
    sizeValue: (logical, allocated) => `${logical} (${allocated} sur le disque)`,
    reclaimable: "Espace libérable",
    atLeast: (size) => `au moins ${size}`,
    complete: "Lumen a tout lu.",
    incomplete: (denied, failed, notScanned) =>
      `Lumen n'a pas tout lu : ${count(denied)} accès refusés, ${count(failed)} échecs, ${count(notScanned)} non analysés.`,
  },
  confirm: {
    title: (n, size) => `Déplacer ${items(n)} (${size}) en quarantaine ?`,
    checked:
      "Lumen vérifie d'abord chaque élément et laisse à sa place tout élément qui a changé depuis l'analyse.",
    restorable:
      "Vous pouvez tout restaurer depuis la quarantaine pendant au moins 30 jours, jusqu'à leur suppression définitive.",
    nothingLeft:
      "Il ne reste rien à déplacer. Relancez l'analyse pour voir ce que vous pouvez nettoyer.",
    items: "Éléments à déplacer",
    move: "Déplacer en quarantaine",
    moving: "Déplacement…",
    failed: "Lumen n'a pas pu effectuer le nettoyage.",
    planMade: (createdAt) => `Lumen a créé ce plan le ${createdAt}. Il est valable 24 heures.`,
    planHash: "Empreinte du plan",
  },
  done: {
    title: (n, size) => `${items(n)} (${size}) déplacés en quarantaine.`,
    noneMoved: "Lumen n'a rien déplacé.",
    freedNote:
      "L'espace sera libéré lorsque vous les supprimerez définitivement de la quarantaine.",
    openQuarantine: "Ouvrir la quarantaine",
    notMoved: (n) => `${items(n)} non déplacés`,
    left: (n) => `${items(n)} restants de cette analyse`,
    scanAgainToMove: "Pour en déplacer davantage, analysez à nouveau.",
  },
  item: {
    quarantined: "Déplacé en quarantaine. Vous pouvez le restaurer.",
    skipped: (why) => `Non déplacé : ${why}.`,
    failed: (reason, where) => `Échec du déplacement : ${reason} ; ${where}.`,
    failIdentityMismatch: "il a changé depuis l'analyse",
    failInUse: "un programme l'utilise",
    failVolumeUnsupported: "ce disque ne peut pas accueillir une quarantaine",
    failMoveFailed: "le déplacement a échoué",
    failVerificationFailed: "après le déplacement, il ne correspondait pas au plan",
    failOther: "pour une raison inconnue de cette version",
    skipChanged: "il a changé ou ne peut plus être vérifié",
    skipOutsideRoots: "il se trouve hors des dossiers analysés",
    skipNowKeep: "Lumen le conserve désormais",
    skipNowReview: "Lumen vous demande désormais de l'examiner",
    skipReasonsChanged: "les raisons de sa proposition ont changé",
    skipNotPrepared: "Lumen n'a pas pu l'enregistrer pour un déplacement sûr",
    skipManifest: "Lumen n'a pas pu écrire son enregistrement",
    leftSource: "il est intact à son emplacement",
    leftQuarantine: "il est en quarantaine et peut être restauré",
    leftGone: "il a disparu",
  },
  quarantine: {
    title: "Quarantaine",
    atLeast: (size) => `au moins ${size}`,
    columns: { name: "Nom", location: "Emplacement d'origine", size: "Taille", held: "Depuis" },
    loadFailed: "Lumen n'a pas pu charger la quarantaine.",
    empty: "La quarantaine est vide",
    emptyHint:
      "Les éléments nettoyés restent ici pour que vous puissiez les restaurer en cas de changement d'avis.",
    goCleanUp: "Ouvrir Explorer",
    interrupted: (n) =>
      `Un nettoyage s'est interrompu et Lumen ne peut pas déterminer où se trouve${n === 1 ? " cet élément" : `nt ces ${count(n)} éléments`}. Lumen ne les a pas supprimés. Vérifiez leur emplacement :`,
    selection: {
      all: "Tout sélectionner",
      item: (name) => `Sélectionner ${name}`,
      count: (n, size) => `${items(n)} sélectionnés · ${size}`,
      clear: "Effacer la sélection",
      restore: "Restaurer",
      removeAsk: "Supprimer définitivement…",
      removeWarning: (n) =>
        `Cette action supprime définitivement ${items(n)} et est irréversible. Lumen vérifie d'abord que rien ne les utilise.`,
      removeSkips: (n) =>
        `${items(n)} qui ne peuvent qu'être restaurés ${n === 1 ? "reste" : "restent"} en quarantaine.`,
      remove: "Supprimer définitivement",
      working: (done, total) => `Opération… ${count(done)} sur ${count(total)}`,
      restored: (well, total) =>
        well === total
          ? `${items(total)} restaurés.`
          : `${count(well)} sur ${items(total)} restaurés. Les autres sont toujours en quarantaine.`,
      removed: (well, total) =>
        well === total
          ? `${items(total)} supprimés définitivement.`
          : `${count(well)} sur ${items(total)} supprimés définitivement. Lumen n'a rien supprimé d'autre.`,
      restoredStopped: (well, total, untried) =>
        [
          `${count(well)} sur ${items(total)} restaurés, puis une erreur a interrompu Lumen.`,
          untried > 0 ? `Les ${count(untried)} autres n'ont pas été traités.` : null,
          "Les éléments toujours dans la liste restent en quarantaine.",
        ]
          .filter((sentence) => sentence !== null)
          .join(" "),
      unverified: (n) =>
        n === 1
          ? "1 élément restauré ne correspond pas exactement à son enregistrement. Vérifiez-le."
          : `${count(n)} éléments restaurés ne correspondent pas exactement à leur enregistrement. Vérifiez-les.`,
      removedStopped: (well, total, untried) =>
        [
          `${count(well)} sur ${items(total)} supprimés définitivement, puis une erreur a interrompu Lumen.`,
          untried > 0 ? `Les ${count(untried)} autres n'ont pas été traités.` : null,
          "Les éléments toujours dans la liste restent en quarantaine et peuvent être restaurés.",
        ]
          .filter((sentence) => sentence !== null)
          .join(" "),
    },
  },
  held: {
    unknownPath: "(emplacement inconnu)",
    days: (days) => (days === 0 ? "Aujourd'hui" : `${count(days)} jour${days > 1 ? "s" : ""}`),
    restoreOnly: "Restauration uniquement",
    afterFailure:
      "Un déplacement a échoué ; Lumen a conservé cet élément en sécurité ici. Vous pouvez uniquement le restaurer.",
    restore: "Restaurer",
    removeForGoodAsk: "Supprimer définitivement…",
    removeForGoodWarning:
      "Cette action est irréversible. Lumen vérifie d'abord que rien n'utilise cet élément.",
    removeForGood: "Supprimer définitivement",
    failed: "Lumen n'a pas pu effectuer cette action. Réessayez.",
  },
  restore: {
    restored: "Restauré à son emplacement d'origine.",
    parentReplaced: "Son dossier a été recréé depuis.",
    unverified: "Il ne correspond pas exactement à son enregistrement. Vérifiez-le.",
    notRestored: (why) => `Lumen n'a pas pu le restaurer. Il reste en quarantaine. ${why}.`,
    nameTaken:
      "Un élément porte désormais son nom à l'emplacement d'origine. Déplacez-le et réessayez.",
    originUnreachable: "Lumen ne peut pas accéder à son dossier d'origine en toute sécurité.",
    itemChanged: "Il a changé pendant son séjour en quarantaine.",
    moveFailed: "Le déplacement vers son emplacement d'origine a échoué.",
    notRestorable: "Lumen ne peut pas restaurer cet élément dans son état actuel.",
  },
  finalize: {
    finalized: "Supprimé définitivement.",
    foundAtSource: "Il est déjà revenu à son emplacement d'origine. Lumen n'a rien supprimé.",
    notFound:
      "Lumen n'a pas trouvé cet élément en quarantaine. S'il se trouvait sur un autre disque, connectez-le et réessayez. Lumen n'a rien supprimé.",
    failed: (what) => `Lumen n'a pas pu le supprimer entièrement. ${what}.`,
    removedNothing: "Aucun élément n'a été supprimé",
    removedPartly: "Une partie a disparu. Réessayez pour terminer.",
    removedItem: "L'élément a disparu ; seul son enregistrement subsiste",
    maybeInUse:
      "Il est peut-être utilisé, ou Lumen ne peut pas le vérifier ; rien n'a donc été supprimé. Fermez les applications qui l'utilisent et réessayez.",
    notFinalizable: "Vous pouvez uniquement restaurer cet élément.",
  },
  apps: {
    title: "Apps",
    desktopOnly: "Lumen affiche les apps installées dans l'app de bureau.",
    reading: "Lecture des apps installées…",
    readAgain: "Relire",
    loadFailed: "Lumen n'a pas pu lire les apps installées.",
    search: "Rechercher des apps",
    showAll: (n: number) => `Afficher les dépendances et éléments masqués (${n})`,
    none: "Aucune app ne correspond.",
    protected: "Fait partie du système",
    protectedTip: "Lumen ne supprime jamais ceci.",
    partial: (names: string) =>
      `Certaines listes n'ont été lues qu'en partie, des apps peuvent donc manquer : ${names}.`,
    readOnly:
      "Cette liste est lue sur votre ordinateur et y reste. Une app est déplacée uniquement après votre confirmation.",
    readOnlyAi:
      "Tant que l’analyse par IA est activée, le nom, l’éditeur et la version de ces apps lui sont envoyés, sans chemin. Une app est déplacée uniquement après votre confirmation.",
    move: {
      action: "Déplacer en quarantaine…",
      title: (name: string) => `Déplacer « ${name} » en quarantaine ?`,
      moving: (name: string) => `Déplacement de « ${name} » en quarantaine…`,
      checking: "Vérification de l’app…",
      failed:
        "Lumen n’a pas pu vérifier ou déplacer l’app. Vérifiez la quarantaine avant de réessayer.",
      refused: "Cette app ne peut pas aller en quarantaine",
      notFound: "Cette app n’est plus dans la liste. Relisez la liste.",
      ambiguous: "Plusieurs apps ont cet identifiant. Lumen ne peut pas en choisir une.",
      unsupported: "Le déplacement des apps en quarantaine est disponible uniquement sur macOS.",
      size: "Taille de l’app",
      atLeast: (size: string) => `Au moins ${size}`,
      expires: (date: string) => `Cet aperçu expire le ${date}, 24 heures après sa création.`,
      moved: (name: string) => `« ${name} » a été déplacée en quarantaine.`,
      noSpace: "L’espace se libère quand vous supprimez définitivement l’app de la quarantaine.",
      changed: "L’app a changé. Fermez cet aperçu et vérifiez-la à nouveau.",
      failedMove: "Lumen n’a pas pu terminer le déplacement de l’app.",
      quit: "Quitter",
      quitHint:
        "Demande à l’app de se fermer, comme son propre menu Quitter. Si du travail n’est pas enregistré, elle vous le demande d’abord.",
      quitAsked:
        "Lumen a demandé à l’app de se fermer. Si elle propose d’enregistrer, répondez dans l’app.",
      stillRunning:
        "Cette app est toujours en cours d’exécution. Quittez-la et vérifiez à nouveau.",
      quitItself: "Lumen ne peut pas se quitter lui-même d’ici.",
      quitFailed:
        "Lumen n’a pas pu demander à l’app de se fermer. Quittez-la vous-même et vérifiez à nouveau.",
      recheck: "Vérifier à nouveau",
      notes: {
        goes_to_quarantine:
          "Vous pouvez la restaurer avant de la supprimer définitivement. Lumen ne la supprime jamais automatiquement.",
        library_data_stays:
          "Ses données dans ~/Library restent en place. Après une nouvelle analyse, vous pouvez les examiner dans Explorer.",
      },
      locations: {
        source: "L’app est toujours à son emplacement d’origine.",
        quarantine: "L’app est en quarantaine. Ouvrez la quarantaine pour la vérifier.",
        gone: "Lumen n’a pas pu confirmer où se trouve l’app. Vérifiez la quarantaine avant de réessayer.",
      },
      reasons: {
        not_in_applications: "Elle n’est pas directement dans /Applications ou ~/Applications.",
        protected: "Elle fait partie de macOS. Lumen la laisse en place.",
        not_a_bundle: "Ce n’est pas un dossier d’app, ou c’est un lien.",
        owner_unknown: "Lumen n’a pas pu vérifier qui possède cette app.",
        not_owned_by_user:
          "Elle appartient à un autre compte. Utilisez son installateur ou l’App Store pour la supprimer.",
        services_unknown: "Lumen n’a pas pu vérifier tous les éléments en arrière-plan.",
        service_linked:
          "Un élément en arrière-plan est lié à cette app. Utilisez son propre outil de désinstallation.",
        helpers_unknown: "Lumen n’a pas pu vérifier ses programmes auxiliaires en arrière-plan.",
        has_helpers:
          "Elle a des programmes auxiliaires en arrière-plan. Utilisez son propre outil de désinstallation.",
        running_unknown: "Lumen n’a pas pu vérifier si cette app est en cours d’exécution.",
        running: "Cette app est en cours d’exécution. Quittez-la et vérifiez à nouveau.",
        store_unknown: "Lumen n’a pas pu vérifier si ce disque dispose d’un espace de quarantaine.",
        no_store_on_volume: "Ce disque n’a pas d’espace de quarantaine.",
      },
    },
    ai: {
      explaining: "L'analyse par IA décrit les apps…",
      note: (purpose: string, confidence: string, impact: string) =>
        `IA : probablement ${purpose} (${confidence}). ${impact}`,
      withConfidence: (impact: string, confidence: string) => `${impact} (${confidence})`,
      purposes: {
        web_browser: "un navigateur web",
        communication: "une app de communication",
        media: "une app multimédia",
        office_or_productivity: "une app de bureautique ou de productivité",
        development_tool: "un outil de développement",
        design_or_creative: "une app de design ou de création",
        game: "un jeu",
        security_or_banking: "un logiciel de sécurité ou bancaire",
        system_or_driver: "un composant système ou un pilote",
        cloud_sync_or_backup: "une app de synchronisation cloud ou de sauvegarde",
        utility: "un utilitaire",
        runtime_or_library: "un environnement d'exécution ou une bibliothèque",
        unknown: "quelque chose qu'elle n'a pas pu déterminer",
      },
      impacts: {
        only_this_app: "La supprimer ne devrait rien affecter d'autre.",
        other_software_depends: "D'autres logiciels en dépendent probablement.",
        stops_background_activity: "La supprimer arrête probablement son activité en arrière-plan.",
        security_or_access:
          "La supprimer peut empêcher la sécurité, la banque ou la connexion de fonctionner.",
        system_function: "La supprimer peut empêcher une fonction du système de fonctionner.",
        unknown: "Elle n'a pas pu déterminer ce que la suppression affecterait.",
      },
    },
    origins: {
      mac_app: "App",
      mac_installer: "Paquet d'installation",
      homebrew: "Homebrew",
      homebrew_cask: "Homebrew Cask",
      windows_program: "Programme",
      msix: "Paquet du Store",
      dpkg: "dpkg",
      rpm: "RPM",
      pacman: "pacman",
      flatpak: "Flatpak",
      snap: "Snap",
      other: "Autre",
    },
  },
  settings: {
    ...en.settings,
    title: "Paramètres",
    appearance: {
      title: "Thème",
      system: "Système",
      light: "Clair",
      dark: "Sombre",
      failed: "Lumen n'a pas pu modifier l'apparence de la fenêtre.",
    },
    permissions: {
      title: "Autorisations",
      fda: "Accès complet au disque",
      fdaIntro:
        "Vous pouvez vérifier l'accès dans les réglages système de macOS. Si vous avez déjà autorisé Lumen, ignorez ce guide. Certains dossiers protégés peuvent rester illisibles malgré l'autorisation.",
      steps: "Afficher les étapes de configuration",
      step1: "Ouvrez Réglages système.",
      step2: "Accédez à Confidentialité et sécurité › Accès complet au disque.",
      step3: "Activez Lumen. S'il est déjà activé, laissez-le ainsi.",
      step4:
        "Si vous avez modifié ce réglage, quittez complètement Lumen, rouvrez-le et lancez une nouvelle analyse.",
      open: "Ouvrir Réglages système",
      openFailed: "Lumen n'a pas pu ouvrir Réglages système. Ouvrez-le depuis le menu Apple.",
      access: "Accès aux dossiers",
      accessIntro:
        "Votre ordinateur peut bloquer l'accès de Lumen à certains dossiers. Lumen lit tout de même chaque dossier autorisé.",
      unread:
        "Lumen signale un dossier illisible comme non lu, jamais comme vide, et n'y propose aucun élément.",
      helper: {
        title: "Assistant administrateur",
        intro:
          "Sert à nettoyer ce qui demande les droits d’administrateur, comme les apps installées par un autre compte. Les règles de sécurité de Lumen restent les mêmes, et tout ce qui est déplacé peut être restauré depuis la Quarantaine.",
        notYet: "Pour l’instant, il vérifie seulement la connexion ; il ne déplace rien.",
        off: "Désactivé.",
        waiting:
          "Autorisez Lumen dans Réglages Système › Général › Ouverture. Un mot de passe administrateur est demandé.",
        on: (version: string) => `Activé. L’assistant répond (version ${version}).`,
        enable: "Activer",
        disable: "Désactiver",
        openLoginItems: "Ouvrir Ouverture",
        changeFailed: "Lumen n’a pas pu modifier l’assistant.",
        problems: {
          outdated:
            "Activé, mais l’assistant vient d’une autre version de Lumen. Désactivez-le puis réactivez-le.",
          unsigned:
            "Cette copie de Lumen n’est pas signée avec un Developer ID ; elle ne peut pas utiliser l’assistant.",
          no_answer: "Activé, mais l’assistant ne répond pas. Désactivez-le puis réactivez-le.",
          not_found: "Lumen ne trouve pas son assistant. Réinstallez Lumen.",
          mismatch:
            "La signature de l’assistant ne correspond pas à ce Lumen. Désactivez-le puis réactivez-le.",
          failed: "Lumen n’a pas pu vérifier l’assistant.",
        },
      },
    },
    about: {
      title: "À propos",
      version: "Version",
      local: "Sur cet appareil",
      localBody:
        "Les analyses, décisions et éléments en quarantaine restent sur cet ordinateur. Le contenu des fichiers n'est jamais envoyé. Lorsque l'analyse par IA est activée, seuls des détails comme les chemins et les tailles sont transmis. Le nom, l'éditeur et la version des apps installées sont aussi transmis.",
      update: {
        title: "Mises à jour",
        checking: "Recherche d'une nouvelle version…",
        upToDate: "Lumen est à jour.",
        available: (version) => `La version ${version} est disponible.`,
        packageOnly:
          "Cette copie a été installée depuis un paquet : téléchargez la nouvelle version sur zaewc.github.io/Lumen.",
        checkFailed: "Lumen n'a pas pu rechercher de mise à jour.",
        check: "Vérifier maintenant",
        install: "Mettre à jour et redémarrer",
        installing: "Mise à jour…",
        installingNote:
          "Lumen télécharge la nouvelle version, vérifie sa signature, l'installe et redémarre.",
        installFailed: "La mise à jour n'a pas été installée. Cette version reste inchangée.",
        waitForItems:
          "Vous pourrez mettre à jour une fois les déplacements et suppressions terminés.",
        sidebar: (version) => `Version ${version} disponible`,
        notice: (version) => `Lumen ${version} est disponible.`,
        noticeAction: "Voir la mise à jour",
        later: "Plus tard",
      },
    },
  },
  whatIs: (term) => `Que signifie « ${term} » ?`,
  glossary: {
    verdicts: {
      title: "Comment Lumen prend ses décisions",
      body: "Lumen prend une décision pour chaque élément à l'aide de règles de sécurité fixes, sans deviner. « Proposé » : par exemple, les caches recréés par leur application ou outil. « Nettoyer avec la commande dédiée » : seuls les outils concernés devraient supprimer ces éléments ; Lumen vous montre leur commande. « À examiner » : les éléments volumineux incertains ou les restes d'applications désinstallées ; ils ne sont jamais déplacés automatiquement. « Conservé » : les éléments intouchables, comme les fichiers système, les éléments utilisés, les données d'applications ou les petits éléments non reconnus. En cas de doute, Lumen ne déplace rien.",
    },
    quarantine: {
      title: "Quarantaine",
      body: "La quarantaine est un espace de conservation sûr. Le nettoyage y déplace les éléments au lieu de les supprimer. Vous pouvez les restaurer à leur emplacement initial jusqu'à leur suppression définitive. Les éléments en quarantaine occupent toujours de l'espace disque.",
    },
  },
  reviewReasons: {
    unknownOwner: "Aucune application ni aucun outil n'a été identifié comme propriétaire.",
    sharedResource:
      "Cet élément peut partager son stockage avec d'autres fichiers ou être lié à des éléments externes.",
    staleEvidence:
      "Certains éléments justificatifs sont anciens. Relancez l'analyse avant de décider.",
    uncertainSize: "L'espace qui serait libéré par la suppression est incertain.",
  },
  reasons: {
    location: (name) => `Conservé : cet élément se trouve dans un emplacement protégé (${name}).`,
    rules: {
      "default.review": "À examiner : Lumen ne reconnaît pas cet élément.",
      "default.keep-small": "Conservé : trop petit pour justifier un examen.",
      "protect.os-protected": "Conservé : le système d'exploitation protège cet élément.",
      "protect.system-file": "Conservé : Windows identifie cet élément comme fichier système.",
      "protect.cloud-backed":
        "Conservé : il est synchronisé avec un service cloud ; sa suppression ici pourrait le supprimer partout.",
      "protect.in-use": "Conservé : une application en cours d'exécution l'utilise.",
      "protect.app-running":
        "Conservé : l'application à laquelle il appartient est en cours d'exécution. Quittez-la, puis relancez l'analyse pour le nettoyer.",
      "protect.in-use-partial":
        "Conservé : aucune application visible par Lumen ne l'utilise, mais Lumen n'a pas pu vérifier toutes les applications.",
      "protect.in-use-unknown": "Conservé : Lumen n'a pas vérifié si un élément l'utilise.",
      "protect.not-observed": "Conservé : l'analyse n'a pas observé cet élément.",
      "protect.incomplete-scan":
        "Conservé : Lumen n'a pas pu le lire entièrement ; son contenu est donc inconnu.",
      "protect.location-data":
        "Conservé : Lumen n'a pas pu vérifier sa liste d'emplacements protégés.",
      "protect.location-context":
        "Conservé : Lumen n'a pas pu vérifier sa liste d'emplacements protégés.",
      "protect.contains-system": "Conservé : il contient une partie du système d'exploitation.",
      "protect.contains-sensitive":
        "Conservé : il contient des mots de passe, des clés ou des sessions de navigateur ; Lumen n'y touche pas.",
      "protect.contains-cloud":
        "Conservé : il contient un dossier synchronisé avec le cloud ; sa suppression ici pourrait supprimer les fichiers partout.",
      "eligible.quarantine-unsupported":
        "À examiner : cet appareil ne peut pas déplacer l'élément en quarantaine de manière réversible.",
      "eligible.frees-nothing": "Conservé : sa suppression ne libérerait aucun espace.",
      "eligible.object-kind":
        "À examiner : il s'agit d'un lien ou d'un fichier spécial, pas de données.",
      "eligible.in-use-partial":
        "Aucune application visible par Lumen ne l'utilise. Lumen n'a pas pu vérifier les applications système ; une nouvelle vérification aura lieu juste avant le déplacement.",
      "eligible.in-use-partial-single-user":
        "Aucune application visible par Lumen ne l'utilise. Vous êtes la seule personne à utiliser cet ordinateur ; Lumen vérifiera les applications système juste avant le déplacement.",
      "classify.devkb-regenerable": "Proposé : l'outil qui l'a créé peut le recréer.",
      "classify.devkb-tool-action":
        "À examiner : seul le nettoyage intégré à l'outil devrait le supprimer, et cette action est irréversible.",
      "classify.devkb-not-regenerable": "À examiner : rien ne peut le recréer.",
      "classify.devkb-self-managed": "Conservé : l'outil qui l'a créé le nettoie automatiquement.",
      "classify.devkb-unknown":
        "À examiner : Lumen n'a pas pu charger les informations correspondantes sur l'outil.",
      "classify.vendor-declared-cache":
        "À examiner : son éditeur indique que vous pouvez le supprimer.",
      "classify.browser-profile":
        "Conservé : il contient des sessions, cookies ou données de navigation ; Lumen n'y touche pas.",
      "classify.app-cache":
        "Proposé : il s'agit d'un cache d'application que celle-ci peut recréer.",
      "classify.app-state":
        "Conservé : il contient des données, réglages ou documents de l'application ; Lumen n'y touche pas.",
      "classify.orphan": "À examiner : l'application à laquelle il appartient semble désinstallée.",
      "jev.demote": "À examiner : l'analyse par IA recommande la prudence.",
    },
  },
};
