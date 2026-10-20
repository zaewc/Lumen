import type { ToolCommandView } from "@lumen/api-client";

/** Characters a POSIX shell reads literally in an unquoted word. */
const PLAIN_WORD = /^[A-Za-z0-9_\-./:=@%+,]+$/;

/**
 * One word for a POSIX shell: as is when every character is plainly literal,
 * otherwise single-quoted, with each `'` written as `'\''`. So a copied
 * command means exactly the argument vector the knowledge base names.
 */
export function shellWord(word: string): string {
  return PLAIN_WORD.test(word) ? word : `'${word.replaceAll("'", "'\\''")}'`;
}

/**
 * A tool's cleanup command as one line to copy: the program and its
 * arguments, each quoted as needed. Lumen only shows it; it never runs it.
 */
export function commandLine(command: ToolCommandView): string {
  return [command.program, ...command.args].map(shellWord).join(" ");
}
