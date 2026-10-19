import { requireNativeModule } from "expo-modules-core";
import { PermissionsAndroid, Platform } from "react-native";

/** One photo, as the native side lists it: an identifier and a display URI. */
export interface Photo {
  readonly id: string;
  readonly uri: string;
  readonly width: number;
  readonly height: number;
}

/** Whether the app may read the photo library. */
export type Access = "granted" | "limited" | "denied" | "undetermined";

interface LumenPhotos {
  requestAccess?(): Promise<Access>;
  count(): number;
  page(offset: number, limit: number): Photo[];
}

const native = requireNativeModule<LumenPhotos>("LumenPhotos");

/** Asks for read access to the photo library. */
export async function requestAccess(): Promise<Access> {
  if (Platform.OS === "android") {
    const result = await PermissionsAndroid.request("android.permission.READ_MEDIA_IMAGES");
    return result === PermissionsAndroid.RESULTS.GRANTED ? "granted" : "denied";
  }
  return native.requestAccess ? native.requestAccess() : "denied";
}

/** How many images the library holds (newest first in pages). */
export function count(): number {
  return native.count();
}

/** `limit` images starting at `offset`, newest first. */
export function page(offset: number, limit: number): Photo[] {
  return native.page(offset, limit);
}
