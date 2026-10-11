import { command } from "./bridge";
import type { HostProfile } from "./dto/host";

/** `host_profile`: read-only host facts (both apps register this command). */
export function hostProfile(): Promise<HostProfile> {
  return command<HostProfile>("host_profile");
}
