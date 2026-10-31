# Android Gradle build outputs

- Date: 2026-10-31
- Question: which parts of an Android Gradle module's build folders, the native
  staging folder `.cxx` and the per-project `.gradle` folder do the owning tools
  identify as regenerable intermediates, as opposed to build results?
- Outcome: match only `build/intermediates` beside a scanned regular `build.gradle`
  or `build.gradle.kts`, as the `android-gradle-project-build-cache` entry. Leave
  everything else out.

## Sources

- Android Gradle plugin (AGP) API,
  [`Artifact.Category`](https://developer.android.com/reference/tools/gradle-api/com/android/build/api/artifact/Artifact.Category):
  "Defines the kind of artifact type. this will be used to determine the output file
  location". AGP's source,
  [`Artifact.kt`](https://android.googlesource.com/platform/tools/base/+/refs/heads/mirror-goog-studio-main/build-system/gradle-api/src/main/java/com/android/build/api/artifact/Artifact.kt),
  describes each category: `GENERATED` is "Generated files that are meant to be
  visible to users from the IDE", `INTERMEDIATES` is "Intermediates files produced by
  tasks", `OUTPUTS` is "output files going into the outputs folder. This is the
  result of the build", and `REPORTS` is "Report files for tests and lint".
- [`ArtifactTypeUtil.kt`](https://android.googlesource.com/platform/tools/base/+/refs/heads/mirror-goog-studio-main/build-system/gradle-core/src/main/java/com/android/build/gradle/internal/scope/ArtifactTypeUtil.kt)
  places an artifact at `<build dir>/<category name in lowercase>/<artifact folder>`,
  so `INTERMEDIATES` lands in `build/intermediates`.
- AGP's [`Cmake`](https://developer.android.com/reference/tools/gradle-api/com/android/build/api/dsl/Cmake)
  and [`NdkBuild`](https://developer.android.com/reference/tools/gradle-api/com/android/build/api/dsl/NdkBuild)
  DSL references call the module's `build/` "your project's temporary build/
  directory" whose files "do not persist through clean builds". `.cxx` is the
  "external native build output directory" and "includes other build system files that
  should persist when performing clean builds, such as Ninja build files".
- Gradle's [directory layout](https://docs.gradle.org/current/userguide/directory_layout.html)
  describes `.gradle` and `build` as "transient files Gradle uses to support features
  like incremental builds". It names `.gradle` the "Project-specific cache directory",
  which Gradle cleans itself: version-specific folders unused for 7 days are deleted.

## In and out

- **In:** `build/intermediates` (directory kind only). AGP's source calls it task
  intermediates, and AGP documents `build/` as temporary and cleared by a clean build.
  This covers app modules and React Native library modules under `node_modules`.
- **Out:** `build/outputs` (APKs, AABs, mapping files: "the result of the build"),
  `build/reports`, `build/generated` (meant to be visible to users), the `build` folder
  itself.
- **Out:** `.cxx`. AGP documents it as an output folder meant to persist through clean
  builds, and no official page says it is a cache or that removing it is safe.
- **Out:** `build/tmp`. Gradle's `Task.getTemporaryDir()` says its contents may not be
  kept, and Gradle's source puts it under `build/tmp`. But it is generic to Gradle, so
  labelling it as AGP would be wrong for non-Android projects, and it is small (under
  1 MB here). It could become a separate Gradle entry.
- **Out:** `build/kotlin`. Kotlin's documentation does not name it.
- **Out:** the project's `.gradle` folder. It is self-managed with a separate source and
  is small (at most 25 MB here); it could become a separate `self_managed` entry.

A file named `build.gradle` or `build.gradle.kts`, either one, is metadata evidence of a
Gradle module. No script is opened or executed, and no `build` contents are read.
Custom `buildDir` locations are not interpreted. A rebuild takes minutes and may
download dependencies when the Gradle user-home caches were also evicted.

## Local measurement

Measured read-only with `du` on two React Native projects on the owner's Desktop:

| Location | Size |
| --- | --- |
| `android/app/build/intermediates` (2 projects) | 6.2 GB |
| `node_modules/*/android/build/intermediates` (same projects) | 8.0 GB |
| `android/app/build/outputs` (left out) | 0.49 GB |
| `.cxx` in app and library modules (left out) | about 2.5 GB |

These are discovery sizes, not reclaimable bytes. Coverage, in-use status, hard links,
clones and the policy still decide eligibility. No files were changed.
