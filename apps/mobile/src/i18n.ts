export type Locale = "en" | "fr";

export interface Messages {
  readonly photos: string;
  readonly all: string;
  readonly similar: string;
  readonly loadingPhotos: string;
  readonly photoAccessOff: string;
  readonly photoAccessBody: string;
  readonly openSettings: string;
  readonly loadPhotosFailed: string;
  readonly tryAgain: string;
  readonly noPhotos: string;
  readonly noPhotosBody: string;
  readonly selected: string;
  readonly similarPhotos: string;
  readonly groupCount: (count: string, n: number) => string;
  readonly similarGroupCount: (count: string, n: number) => string;
  readonly stats: string;
  readonly hideStats: string;
  readonly findSimilar: string;
  readonly findingSimilar: string;
  readonly similarFailed: (message: string) => string;
  readonly searchAgain: string;
  readonly noSimilarFound: string;
  readonly stoppedEarly: string;
  readonly checked: string;
  readonly jsFramesPerSecond: string;
  readonly p50: string;
  readonly p95: string;
  readonly dropped: string;
  readonly analyzed: string;
  readonly prints: string;
  readonly grouping: string;
  readonly cancelled: string;
  readonly core: string;
  readonly repeatTo: (count: string) => string;
  readonly repeatFrom: (items: string, photos: string) => string;
  readonly repeatAccessibilityLabel: string;
}

export const MESSAGES: Readonly<Record<Locale, Messages>> = {
  en: {
    photos: "Photos",
    all: "All",
    similar: "Similar",
    loadingPhotos: "Loading photos…",
    photoAccessOff: "Photo access is off",
    photoAccessBody:
      "To see your photos here, allow Lumen to read your photo library in Settings. Lumen changes nothing without asking you first.",
    openSettings: "Open Settings",
    loadPhotosFailed: "Lumen could not load your photos",
    tryAgain: "Try again",
    noPhotos: "No photos yet",
    noPhotosBody: "Photos you take or save show up here.",
    selected: "you selected",
    similarPhotos: "similar photos",
    groupCount: (count, n) => `${count} ${n === 1 ? "group" : "groups"}`,
    similarGroupCount: (count, n) => `${count} ${n === 1 ? "group" : "groups"} of similar photos`,
    stats: "Stats",
    hideStats: "Hide stats",
    findSimilar: "Find similar photos",
    findingSimilar: "Finding similar photos…",
    similarFailed: (message) => `Lumen could not find similar photos: ${message}`,
    searchAgain: "Search again",
    noSimilarFound: "No similar photos found",
    stoppedEarly: "stopped early",
    checked: "checked",
    jsFramesPerSecond: "JS frames/s",
    p50: "p50",
    p95: "p95",
    dropped: "dropped",
    analyzed: "analyzed",
    prints: "prints",
    grouping: "grouping",
    cancelled: "cancelled",
    core: "Core",
    repeatTo: (count) => `Repeat to ${count}`,
    repeatFrom: (items, photos) => `${items} items, repeated from ${photos}`,
    repeatAccessibilityLabel: "Repeat the library to 50,000 items",
  },
  fr: {
    photos: "Photos",
    all: "Toutes",
    similar: "Similaires",
    loadingPhotos: "Chargement des photos…",
    photoAccessOff: "Accès aux photos désactivé",
    photoAccessBody:
      "Pour voir vos photos ici, autorisez Lumen à lire votre photothèque dans les réglages. Lumen ne modifie rien sans vous demander.",
    openSettings: "Ouvrir les réglages",
    loadPhotosFailed: "Lumen n'a pas pu charger vos photos",
    tryAgain: "Réessayer",
    noPhotos: "Pas encore de photos",
    noPhotosBody: "Les photos que vous prenez ou enregistrez apparaîtront ici.",
    selected: "sélectionnées",
    similarPhotos: "photos similaires",
    groupCount: (count, n) => `${count} ${n === 1 ? "groupe" : "groupes"}`,
    similarGroupCount: (count, n) =>
      `${count} ${n === 1 ? "groupe" : "groupes"} de photos similaires`,
    stats: "Statistiques",
    hideStats: "Masquer les statistiques",
    findSimilar: "Rechercher des photos similaires",
    findingSimilar: "Recherche de photos similaires…",
    similarFailed: (message) => `Lumen n'a pas pu trouver de photos similaires : ${message}`,
    searchAgain: "Rechercher à nouveau",
    noSimilarFound: "Aucune photo similaire trouvée",
    stoppedEarly: "arrêtée avant la fin",
    checked: "vérifiées",
    jsFramesPerSecond: "Images JS/s",
    p50: "p50",
    p95: "p95",
    dropped: "perdues",
    analyzed: "analysées",
    prints: "empreintes",
    grouping: "regroupement",
    cancelled: "annulée",
    core: "Noyau",
    repeatTo: (count) => `Répéter jusqu'à ${count}`,
    repeatFrom: (items, photos) => `${items} éléments, répétés à partir de ${photos}`,
    repeatAccessibilityLabel: "Répéter la photothèque jusqu'à 50 000 éléments",
  },
};

/** Follow the device language when available; other languages use English. */
export function detectLocale(tag: string = Intl.DateTimeFormat().resolvedOptions().locale): Locale {
  const [language = ""] = tag.toLowerCase().split(/[-_]/);
  return language === "fr" ? "fr" : "en";
}

export const locale = detectLocale();
export const messages = MESSAGES[locale];
