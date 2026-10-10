// Consuming bundlers (Next.js) ship their own declaration; this one covers standalone `tsc` checks.
declare module "*.module.css" {
  const classes: Readonly<Record<string, string>>;
  export default classes;
}
