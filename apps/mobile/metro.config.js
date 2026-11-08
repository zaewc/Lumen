import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { getDefaultConfig } from "expo/metro-config";

const projectRoot = dirname(fileURLToPath(import.meta.url));
const config = getDefaultConfig(projectRoot);

config.watchFolders = [...config.watchFolders, resolve(projectRoot, "../../fonts")];

export default config;
