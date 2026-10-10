// Password rules for the preview vault step. Nothing is stored; real setup will hand the password to the native vault.
export const MIN_PASSWORD_LENGTH = 10;
/** Lowest strength score that can continue: 2 is "Fair". */
export const MIN_STRENGTH_SCORE = 2;

export type PasswordStrength = { score: 0 | 1 | 2 | 3 | 4; label: string; tone: "danger" | "warning" | "accent" | "success" };

/** Rough strength estimate from length and character variety; real setup will use a proper estimator. */
export function passwordStrength(password: string): PasswordStrength {
  if (!password) return { score: 0, label: "Not set", tone: "accent" };
  const variety = [/[a-z]/, /[A-Z]/, /\d/, /[^A-Za-z0-9]/].filter((pattern) => pattern.test(password)).length;
  const lengthPoints = password.length >= 16 ? 2 : password.length >= MIN_PASSWORD_LENGTH ? 1 : 0;
  const raw = lengthPoints + (variety >= 3 ? 2 : variety >= 2 ? 1 : 0);
  const score = Math.min(raw, 4) as PasswordStrength["score"];
  if (score <= 1) return { score, label: "Weak", tone: "danger" };
  if (score === 2) return { score, label: "Fair", tone: "warning" };
  if (score === 3) return { score, label: "Good", tone: "accent" };
  return { score, label: "Strong", tone: "success" };
}

/** Why the passwords can't be used yet, or undefined when they can. */
export function passwordProblem(password: string, confirmation: string): string | undefined {
  if (password.length < MIN_PASSWORD_LENGTH) return `Use at least ${MIN_PASSWORD_LENGTH} characters.`;
  if (passwordStrength(password).score < MIN_STRENGTH_SCORE) return "Too weak. Add length or mix letters, numbers, and symbols.";
  if (password !== confirmation) return "Passwords do not match.";
  return undefined;
}
