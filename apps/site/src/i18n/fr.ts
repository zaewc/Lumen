import { REPO_URL } from "../data/release";
import type { Messages } from "./types";

export const fr: Messages = {
  lang: "fr",
  language: "Langue",
  description:
    "Comprenez le stockage de vos appareils. Des règles de sécurité, avec une analyse par IA facultative, vous aident à décider quoi supprimer. Lumen est un logiciel pré-alpha.",
  ogDescription:
    "Comprendre le stockage de vos appareils. La sécurité avant l'espace récupéré. Local, libre et pré-alpha.",
  skip: "Aller au contenu",
  nav: { label: "Site", docs: "Documentation" },
  hero: {
    headline: ["Comprendre votre stockage", "sur tous vos appareils."],
    lead: "Une application gratuite qui repère ce qui remplit votre disque et libère de l'espace en toute sécurité, avec un second avis de l'IA. Les fichiers nettoyés passent d'abord en quarantaine : vous pouvez les restaurer.",
    download: "Voir les versions",
    requirements: "macOS 13+ · Windows 10 22H2+ · Linux x86_64",
  },
  showcase: {
    main: "Page Explorer de Lumen avec des exemples : répartition du stockage selon la décision, 9,9 Go proposés dans les outils de développement et les caches d'applications, et commandes pour nettoyer les éléments à l'aide de leur outil.",
  },
  howTo: {
    title: "Comment l'utiliser",
    steps: [
      {
        title: "Téléchargez et ouvrez l'application",
        text: "Choisissez ci-dessous le fichier adapté à votre ordinateur et installez-le. À la première ouverture de Lumen, votre ordinateur vous demandera une fois de l'autoriser.",
      },
      {
        title: "Analysez votre disque",
        text: "Cliquez sur Analyser le disque. Lumen indique ce qui occupe de l'espace et prend une décision pour chaque élément. L'analyse ne modifie rien.",
      },
      {
        title: "Nettoyez les éléments proposés",
        text: "Vérifiez les éléments marqués comme proposés, puis cliquez sur Nettoyer. Lumen les déplace en quarantaine, où vous pourrez les restaurer.",
      },
    ],
  },
  features: {
    title: "Ce que fait Lumen",
    items: [
      {
        title: "Repère ce qui occupe de l'espace",
        text: "Caches d'applications, anciens installateurs et données de compilation laissées par les outils de développement. L'analyse n'ouvre jamais les fichiers.",
      },
      {
        title: "Explique chaque décision",
        text: "Qui a créé un élément, s'il peut être recréé et si quelque chose l'utilise encore : les faits déterminent s'il faut le conserver, l'examiner ou le nettoyer.",
      },
      {
        title: "L'IA apporte un second avis",
        text: "Lorsque l'analyse par IA est activée, les chemins, tailles et anciennetés des éléments sont envoyés au serveur Jeb de l'établissement, dont l'exploitant peut les conserver. Le contenu des fichiers n'est jamais envoyé. L'IA réévalue les éléments proposés au nettoyage et indique ce que pourraient être ceux à examiner.",
      },
      {
        title: "Nettoie après votre confirmation",
        text: "Les fichiers sont placés en quarantaine pour que vous puissiez les restaurer. Certains éléments, comme le stockage d'un outil de développement, ne devraient être nettoyés que par cet outil : Lumen n'exécute sa commande qu'après vérification de votre part. Cette suppression est irréversible.",
      },
    ],
  },
  trust: {
    items: [
      {
        headline: ["Vos fichiers d'abord.", "L'espace libre ensuite."],
        text: "Aucune suppression automatique. Restaurez les fichiers en quarantaine tant que vous ne les avez pas supprimés définitivement.",
      },
      {
        headline: ["Examinez les faits.", "Pas de suppositions."],
        text: "Ce qu'est un fichier. Ce qui l'utilise encore. En cas d'incertitude, il reste à sa place.",
      },
    ],
    more: "Lire l'architecture",
  },
  download: {
    title: "Tous les téléchargements et l'installation",
    noScript: [
      "Les fichiers proviennent des ",
      { link: "versions GitHub", href: `${REPO_URL}/releases` },
      ".",
    ],
    checking: "Vérification des versions…",
    thisComputer: "Pour cet ordinateur",
    installer: "Installateur",
    requirements: {
      mac: "macOS 13 ou ultérieur · Apple silicon et Intel",
      windows:
        "Windows 10 22H2 ou ultérieur, Windows 11 · 64 bits · aucun droit administrateur requis",
      linux: "x86_64 · Ubuntu 24.04 ou ultérieur, ou équivalent",
    },
  },
  files: {
    summary: "Autres fichiers : ligne de commande, sommes de contrôle, SBOM",
    intro: [
      "L'outil en ligne de commande ",
      { code: "lumen" },
      " est fourni sous la forme ",
      { code: "lumen-<target>.tar.gz" },
      " (macOS sur Apple silicon, Linux) et ",
      { code: ".zip" },
      " (Windows). Les fichiers ",
      { code: ".cdx.json" },
      " répertorient les composants de chaque compilation (SBOM).",
    ],
  },
  verify: {
    summary: "Vérifier un téléchargement",
    introBefore: ["Placez "],
    introAfter: [
      " à côté du fichier téléchargé et exécutez cette commande dans ce dossier. Le résultat doit indiquer ",
      { code: "OK" },
      ".",
    ],
    windowsNote:
      " · comparez le résultat à la ligne du fichier dans SHA256SUMS ; la casse n'a pas d'importance.",
    provenance: "Provenance de la compilation",
    provenanceNote: [
      " · vérifie que le processus de publication de ce dépôt a créé le fichier. Nécessite ",
      { link: "GitHub CLI", href: "https://cli.github.com/" },
      ".",
    ],
  },
  install: {
    title: "Première ouverture de Lumen",
    intro:
      "Lumen n'est pas encore signé : macOS et Windows bloquent donc son premier lancement. Autorisez-le une fois ; il s'ouvrira normalement par la suite.",
    mac: {
      steps: [
        [
          "Ouvrez le fichier ",
          { code: ".dmg" },
          " et faites glisser Lumen dans ",
          { ui: "Applications" },
          ".",
        ],
        [
          "Ouvrez Lumen. Lorsque macOS indique qu'il ne peut pas s'ouvrir, cliquez sur ",
          { ui: "Terminé" },
          ".",
        ],
        [
          "Dans ",
          { ui: "Réglages système › Confidentialité et sécurité" },
          ", faites défiler jusqu'à ",
          { ui: "Sécurité" },
          ", cliquez sur ",
          { ui: "« Ouvrir quand même »" },
          " et confirmez avec votre mot de passe ou Touch ID.",
        ],
        [
          "Rouvrez Lumen et cliquez sur ",
          { ui: "Ouvrir quand même" },
          ". Il s'ouvrira normalement par la suite.",
        ],
      ],
      aside: [
        "Sous macOS 13 et 14, vous pouvez aussi faire un clic droit en maintenant Contrôle, puis choisir ",
        { ui: "Ouvrir" },
        ". À partir de macOS 15, suivez les étapes ci-dessus.",
      ],
    },
    fda: {
      summary: "Accès complet au disque sous macOS",
      intro:
        "Cet accès permet aussi à Lumen d'analyser des dossiers protégés, comme les données de Mail, Messages et d'autres applications. Sans lui, ces dossiers restent illisibles et Lumen n'y touche pas.",
      steps: [
        [
          "Ouvrez ",
          { ui: "Réglages système › Confidentialité et sécurité › Accès complet au disque" },
          ", ou utilisez le bouton dans les réglages de Lumen.",
        ],
        [
          "Activez Lumen. S'il n'apparaît pas dans la liste, ajoutez-le depuis Applications avec ",
          { ui: "+" },
          ".",
        ],
        ["Quittez et rouvrez Lumen, puis relancez l'analyse."],
      ],
      aside: [
        "Sans signature, macOS peut considérer une version mise à jour de Lumen comme une autre application. Si des dossiers deviennent illisibles après une mise à jour, retirez Lumen de la liste avec ",
        { ui: "−" },
        " puis ajoutez-le à nouveau.",
      ],
    },
    windows: {
      steps: [
        [
          "Si votre navigateur vous avertit au sujet du téléchargement, choisissez ",
          { ui: "Conserver" },
          " dans le menu du téléchargement.",
        ],
        [
          "Lancez l'installateur. Dans l'avertissement ",
          { ui: "Windows a protégé votre ordinateur" },
          ", cliquez sur ",
          { ui: "Informations complémentaires" },
          ", puis sur ",
          { ui: "Exécuter quand même" },
          ".",
        ],
        [
          "Ouvrez Lumen depuis le menu Démarrer avec un compte standard. En tant qu'administrateur, Lumen ne déplacera pas de fichiers.",
        ],
      ],
    },
    linux: {
      aside: [
        "Si l'AppImage ne démarre pas, FUSE 2 est peut-être nécessaire (Ubuntu 24.04 : ",
        { code: "sudo apt install libfuse2t64" },
        "). Lancez Lumen avec un compte standard ; en tant que superutilisateur, il ne déplacera pas de fichiers.",
      ],
    },
  },
  footer: {
    source: "Dépôt GitHub",
    releases: "Toutes les versions",
    contact: "Contact",
  },
  copy: "Copier",
  client: {
    dateLocale: "fr-FR",
    download: "Choisir un téléchargement pour ordinateur",
    downloadFor: "Télécharger pour {platform}",
    viewOnGitHub: "Suivre le développement sur GitHub",
    downloadFromGitHub: "Consulter les versions GitHub",
    getFromGitHub: "Consulter les versions GitHub",
    comingSoonShort: "Aucune version publique pour le moment. ",
    loadFailed: "Les informations de version sont indisponibles. Consultez directement GitHub.",
    checkFailed: "Les informations de version sont indisponibles. ",
    rateLimited:
      "GitHub reçoit trop de requêtes ; cette page n'a pas pu vérifier la dernière version. ",
    releaseNotes: "Notes de version",
    githubReleases: "Versions GitHub",
    allFiles: "Voir tous les fichiers sur GitHub",
    fileHeader: "Fichier",
    sizeHeader: "Taille",
    copied: "Copié",
  },
  notFound: { title: "Page introuvable", home: "Accéder à la page de téléchargement de Lumen" },
};
