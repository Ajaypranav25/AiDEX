import {
  createAgentSession,
  SessionManager,
  Settings,
} from "@oh-my-pi/pi-coding-agent";

function parseArgs(args: string[]) {
  let cwd = process.cwd();
  let modelPattern: string | undefined;
  const prompt: string[] = [];

  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === "--cwd" || argument === "--model") {
      const value = args[index + 1];
      if (!value) throw new Error(`${argument} requires a value`);
      if (argument === "--cwd") cwd = value;
      else modelPattern = value;
      index += 1;
      continue;
    }
    if (argument === "--") {
      prompt.push(...args.slice(index + 1));
      break;
    }
    prompt.push(argument);
  }

  if (prompt.length === 0) throw new Error("run requires a prompt");
  return { cwd, modelPattern, prompt: prompt.join(" ") };
}

async function main() {
  const { cwd, modelPattern, prompt } = parseArgs(process.argv.slice(2));
  const { session, modelFallbackMessage } = await createAgentSession({
    cwd,
    modelPattern,
    settings: Settings.isolated({ "tools.approvalMode": "always-ask" }),
    sessionManager: SessionManager.inMemory(),
    toolNames: ["read", "grep", "glob"],
    restrictToolNames: true,
    enableMCP: false,
    enableLsp: false,
    disableExtensionDiscovery: true,
    customTools: [],
    skills: [],
    rules: [],
    cacheWarming: false,
    autoApprove: false,
  });

  if (modelFallbackMessage) {
    process.stderr.write(`aidex: ${modelFallbackMessage}\n`);
  }

  const unsubscribe = session.subscribe((event) => {
    if (
      event.type === "message_update" &&
      event.assistantMessageEvent.type === "text_delta"
    ) {
      process.stdout.write(event.assistantMessageEvent.delta);
    }
  });

  try {
    await session.prompt(prompt);
    process.stdout.write("\n");
  } finally {
    unsubscribe();
    await session.dispose();
  }
}

main().catch((error: unknown) => {
  const message = error instanceof Error ? error.message : String(error);
  process.stderr.write(`aidex: ${message}\n`);
  process.exitCode = 1;
});
