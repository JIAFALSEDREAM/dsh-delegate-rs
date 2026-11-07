// A startup-only probe. No Agent is created and no model request is made.
export const name = "delegate-runtime-probe";
export const inject = ["ptcRuntime", "sandboxPolicy"];

export function apply(ctx) {
  const run = async () => {
    await ctx.get("loader")?.await();
    const spec = ctx.ptcRuntime.resolve({
      cwd: process.cwd(),
      timeoutMs: 5000,
      sandboxPolicy: { mode: "read-only", workspaceRoot: process.cwd() },
      bindings: [{ global: "probe", functions: { echo: async (value) => value } }],
      program: `
        const fs = await import("node:fs/promises");
        const text = await fs.readFile("marker", "utf8");
        let denied = false;
        try { await fs.writeFile("marker", "changed"); }
        catch (error) { denied = ["EACCES", "EPERM", "EROFS"].includes(error.code); }
        return { read: text, write_denied: denied, rpc: await probe.echo(text) };
      `,
    });
    const result = await ctx.ptcRuntime.run(spec);
    process.stdout.write(`${JSON.stringify(result)}\n`);
    ctx.get("appExit")(result.error ? 1 : 0);
  };
  run().catch((error) => {
    process.stderr.write(`dsh PTC probe: ${error.message}\n`);
    ctx.get("appExit")(1);
  });
}
