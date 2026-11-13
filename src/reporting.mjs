import { existsSync } from "node:fs";
import { randomUUID } from "node:crypto";

export const name = "delegate-reporting";
export const inject = ["agents"];

export function apply(ctx, config) {
  const timers = new Map();
  const clear = (agent) => {
    clearInterval(timers.get(agent));
    timers.delete(agent);
  };
  ctx.on("agent/created", ({ agent }) => {
    const timer = setInterval(() => {
      if (agent.status !== "running" || !existsSync(config.signal)) return;
      clear(agent);
      // Steering is delivered at the next step boundary without aborting active tools.
      // Do not wake an idle agent: its report may already be complete.
      agent.steer(Object.freeze({
        id: randomUUID(),
        role: "user",
        source: Object.freeze({
          kind: "delegate-budget",
          form: "notice",
          summary: "Investigation budget reached; finish the report",
        }),
        content: Object.freeze([Object.freeze({ type: "text", text: config.message })]),
      }));
    }, 50);
    timer.unref();
    timers.set(agent, timer);
  });
  ctx.on("agent/disposed", ({ agent }) => clear(agent));
  ctx.on("dispose", () => {
    for (const agent of timers.keys()) clear(agent);
  });
}
