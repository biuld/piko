/**
 * Vitest setup file: ensures a writable HOME directory for all tests.
 *
 * Without this, tests that create PikoHost (which instantiates SessionManager)
 * fail in sandboxed environments where ~/.piko is not writable.
 *
 * This runs before each test file in the same worker context, so the HOME
 * variable is visible to all test code.
 */
import * as fs from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const home = fs.mkdtempSync(join(tmpdir(), "piko-test-setup-"));
process.env.HOME = home;
