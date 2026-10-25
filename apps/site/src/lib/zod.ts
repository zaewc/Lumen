import { z } from "zod";

// The site allows no eval in its CSP. Disable code generation before schemas
// are constructed, including the capability probe that would report a violation.
z.config({ jitless: true });

export { z };
