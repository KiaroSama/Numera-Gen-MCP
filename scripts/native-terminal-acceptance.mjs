import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

function receipt(id, connection, adapter) {
  return {
    request_id: id,
    connection,
    adapter,
    operation: "generate",
    status: "prepared",
    requested_model: "image-model",
    upstream_model: null,
    outputs: [],
    warnings: [],
    deviations: [],
    errors: [],
    generation_outcome: "not_submitted",
    storage_outcome: "not_started",
    another_submission_may_charge: false,
    created_at: "2026-10-02T00:00:00.000Z",
    updated_at: "2026-10-02T00:00:00.000Z",
  };
}

async function seed(config, modes, kind, { Store, connectionIdentity }) {
  const store = new Store(config.stateDir);
  try {
    for (const mode of modes) {
      const connection = config.connections[mode];
      const saved = store.prepare(
        receipt(mode, mode, connection.adapter),
        mode,
        mode === "identity"
          ? "other-account"
          : await connectionIdentity(mode, connection),
      ).receipt;
      assert.equal(store.admit(mode, mode, 32, 1), true);
      saved.status = "running";
      saved.generation_outcome = "running";
      saved.upstream_job = { id: mode, kind };
      store.update(saved);
      store.release(mode, saved);
    }
  } finally {
    store.close();
  }
}

function configuration(root, connections, configSchema) {
  const config = configSchema.parse({
    schemaVersion: 1,
    defaultConnection: Object.keys(connections)[0],
    outputDir: join(root, "outputs"),
    stateDir: join(root, "state"),
    logging: { directory: join(root, "logs") },
    files: { allowedInputRoots: [join(root, "inputs")] },
    connections,
  });
  config.maxConcurrentRequests = 32;
  config.files.maxAggregateBytes = 4096;
  config.files.maxJournalBytes = 1024 * 1024;
  return config;
}

async function provider(respond, run, bounded) {
  const sockets = new Set();
  const errors = [];
  const server = createServer((request, response) => {
    try {
      respond(request, response);
    } catch (error) {
      errors.push(error);
      response.destroy();
    }
  });
  server.requestTimeout = 5000;
  server.headersTimeout = 5000;
  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.once("close", () => sockets.delete(socket));
  });
  try {
    await bounded(
      new Promise((accept, reject) => {
        server.once("error", reject);
        server.listen(0, "127.0.0.1", accept);
      }),
    );
    const origin = `http://127.0.0.1:${server.address().port}`;
    await run(origin);
    if (errors.length) throw errors[0];
  } finally {
    for (const socket of sockets) socket.destroy();
    if (server.listening)
      await bounded(new Promise((accept) => server.close(accept)));
  }
}

function json(response, value, status = 200) {
  response.writeHead(status, {
    "Content-Type": "application/json; charset=utf-8",
  });
  response.end(JSON.stringify(value));
}

async function call(host, name, arguments_, bounded) {
  host.fixtureSignal.throwIfAborted();
  return bounded(
    host.client.callTool(
      { name, arguments: arguments_ },
      { timeout: 5000, signal: host.fixtureSignal },
    ),
    6000,
  );
}

async function job(host, id, refresh, bounded) {
  const result = await call(
    host,
    "get_job",
    { request_id: id, refresh },
    bounded,
  );
  assert.notEqual(result.isError, true, JSON.stringify(result));
  assert.ok(result.structuredContent);
  return result.structuredContent;
}

async function pair(root, config, context, instances) {
  const file = join(root, "config.json");
  await writeFile(file, JSON.stringify(config), "utf8");
  const hosts = [];
  try {
    // Sequential handshakes preserve the source's initialization schedule.
    for (let index = 0; index < 2; index++) {
      context.signal.throwIfAborted();
      let tail = "";
      let signalReady;
      let rejectReady;
      const ready = new Promise((accept, reject) => {
        signalReady = accept;
        rejectReady = reject;
      });
      // Readiness can reject during connect; observe it before the later await.
      void ready.catch(() => {});
      const controller = new AbortController();
      const timer = setTimeout(
        () =>
          controller.abort(
            new Error("Terminal host exceeded source lifetime."),
          ),
        45000,
      );
      const signal = AbortSignal.any([context.signal, controller.signal]);
      const aborted = () => rejectReady(signal.reason);
      signal.addEventListener("abort", aborted, { once: true });
      const cleanup = () => {
        clearTimeout(timer);
        signal.removeEventListener("abort", aborted);
      };
      let host;
      try {
        host = await context.start(
          file,
          "auto",
          { signal },
          {
            clientInfo: { name: `terminal-host-${index}`, version: "1" },
            env: { NUMERA_LOG_DIR: config.logging.directory },
            onStderr(chunk) {
              tail += chunk.toString("utf8");
              if (tail.length > 1048576) {
                const error = new Error(
                  "Terminal host diagnostic line exceeded one MiB.",
                );
                rejectReady(error);
                controller.abort(error);
                tail = "";
                return;
              }
              for (let end; (end = tail.indexOf("\n")) >= 0;) {
                const line = tail.slice(0, end);
                tail = tail.slice(end + 1);
                try {
                  const event = JSON.parse(line);
                  if (
                    event.component === "MCP" &&
                    event.message === "Stdio server ready."
                  )
                    signalReady(event.run_id);
                } catch {
                  // Non-JSON diagnostics are not a readiness event.
                }
              }
            },
          },
        );
      } catch (error) {
        cleanup();
        throw error;
      }
      host.fixtureSignal = signal;
      host.fixtureCleanup = cleanup;
      hosts.push(host);
      signal.throwIfAborted();
      const runId = await context.bounded(ready);
      assert.match(runId, /^[a-f0-9-]{36}$/);
      assert.equal(instances.has(runId), false);
      instances.add(runId);
    }
    assert.notEqual(hosts[0].pid, hosts[1].pid);
    return hosts;
  } catch (error) {
    await close(hosts, context.closeHost);
    throw error;
  }
}

async function close(hosts, closeHost) {
  const results = await Promise.allSettled(
    hosts.map(async (host) => {
      try {
        await closeHost(host);
        host.fixtureSignal.throwIfAborted();
      } finally {
        host.fixtureCleanup();
      }
    }),
  );
  const failures = results.filter((result) => result.status === "rejected");
  if (failures.length)
    throw new AggregateError(
      failures.map((result) => result.reason),
      "Terminal host cleanup failed.",
    );
}

async function terminal(adapter, context, original) {
  const root = join(context.workspace, `terminal-${adapter}`);
  await mkdir(root);
  const states = [
    "failed",
    "cancelled",
    "incomplete",
    "completed",
    ...(adapter === "gemini-interactions" ? ["budget_exceeded"] : []),
  ];
  const controls = [
    "queued",
    "in_progress",
    "401",
    "429",
    "500",
    "disconnect",
    "malformed",
    "wrong-id",
    "identity",
  ];
  const modes = [...states, ...controls];
  const posts = [];
  const image =
    adapter === "openai-responses"
      ? {
          output: [
            {
              type: "image_generation_call",
              status: "completed",
              result: original.base64,
            },
          ],
        }
      : {
          steps: [
            {
              type: "model_output",
              content: [{ type: "image", data: original.base64 }],
            },
          ],
        };
  await provider(
    (request, response) => {
      const mode = request.url.split("/")[1];
      if (request.method === "POST") {
        posts.push(mode);
        json(response, { id: "probe", status: "completed", ...image });
      } else if (mode === "disconnect") response.destroy();
      else if (mode === "malformed") response.end("not-json");
      else
        json(
          response,
          {
            id: mode === "wrong-id" ? "other" : mode,
            status: states.includes(mode)
              ? mode
              : mode === "queued"
                ? "queued"
                : "in_progress",
            ...(["completed", "incomplete"].includes(mode) ? image : {}),
          },
          ["401", "429", "500"].includes(mode) ? Number(mode) : 200,
        );
    },
    async (origin) => {
      const config = configuration(
        root,
        Object.fromEntries(
          modes.map((mode) => [
            mode,
            {
              adapter,
              baseUrl: `${origin}/${mode}/v1`,
              auth: { type: "none" },
              orchestrationModel: "text",
              defaultModel: "image",
              requestTimeoutMs: 700,
              maxConcurrentRequests: 1,
            },
          ]),
        ),
        context.configSchema,
      );
      const kind =
        adapter === "openai-responses" ? "responses" : "interactions";
      await seed(config, modes, kind, context);
      const outputIds = new Map();
      const instances = new Set();
      for (let round = 0; round < 3; round++) {
        const hosts = await pair(root, config, context, instances);
        try {
          for (const mode of modes) {
            await Promise.all(
              hosts.map((host) =>
                call(
                  host,
                  "get_job",
                  {
                    request_id: mode,
                    refresh: true,
                  },
                  context.bounded,
                ),
              ),
            );
            for (const host of hosts) {
              const current = await job(host, mode, false, context.bounded);
              if (states.includes(mode)) {
                const state = mode === "budget_exceeded" ? "failed" : mode;
                assert.equal(current.generation_outcome, state);
                assert.equal(
                  current.status,
                  mode === "completed"
                    ? "completed"
                    : mode === "incomplete"
                      ? "partial"
                      : mode === "cancelled"
                        ? "cancelled"
                        : "failed",
                );
                if (["completed", "incomplete"].includes(mode)) {
                  assert.equal(current.outputs.length, 1);
                  assert.deepEqual(
                    await readFile(current.outputs[0].path),
                    Buffer.from(original.base64, "base64"),
                  );
                  if (outputIds.has(mode))
                    assert.equal(
                      current.outputs[0].output_id,
                      outputIds.get(mode),
                    );
                  else outputIds.set(mode, current.outputs[0].output_id);
                }
              } else {
                assert.equal(current.status, "running");
                assert.equal(current.upstream_terminal, undefined);
              }
              assert.deepEqual(current.upstream_job, { id: mode, kind });
            }
            const before = posts.length;
            const probe = await call(
              hosts[1],
              "generate_image",
              {
                connection: mode,
                prompt: "admission probe",
                request_id: `probe-${mode}-${round}`,
              },
              context.bounded,
            );
            assert.equal(posts.length - before, states.includes(mode) ? 1 : 0);
            if (states.includes(mode)) assert.notEqual(probe.isError, true);
            const replay = await call(
              hosts[0],
              "generate_image",
              {
                connection: mode,
                prompt: "different fingerprint",
                request_id: mode,
              },
              context.bounded,
            );
            assert.equal(replay.isError, true);
            assert.ok(JSON.stringify(replay).includes("request_id_conflict"));
            assert.equal(posts.length - before, states.includes(mode) ? 1 : 0);
          }
        } finally {
          await close(hosts, context.closeHost);
        }
        process.stderr.write(
          JSON.stringify({
            component: "native-terminal-acceptance",
            adapter,
            round,
            message: "Terminal and polling control round completed.",
            cases: modes.length,
          }) + "\n",
        );
      }
      // Startup run IDs identify six actual process instances even if an OS reuses a PID.
      assert.equal(instances.size, 6);
      assert.equal(posts.length, states.length * 3);
    },
    context.bounded,
  );
}

async function comfy(context, original) {
  const root = join(context.workspace, "terminal-comfy-cancel");
  await mkdir(root);
  const modes = [
    "cancelled",
    "failed",
    "dispatched",
    "false",
    "missing",
    "ambiguous",
    "wrong-id",
    "completed",
  ];
  let cancellations = 0;
  let generations = 0;
  await provider(
    (request, response) => {
      const url = new URL(request.url, "http://fixture");
      const mode = url.searchParams.get("fixture");
      if (request.method === "POST") {
        if (url.pathname.endsWith("/cancel")) {
          cancellations++;
          if (mode === "ambiguous") response.destroy();
          else
            json(
              response,
              mode === "missing" ? {} : { cancelled: mode !== "false" },
            );
        } else {
          generations++;
          json(response, { prompt_id: "probe" });
        }
      } else if (request.url.includes("/object_info"))
        json(response, {
          Text: { input: { required: { text: ["STRING"] } } },
          Save: {
            output_node: true,
            input: { required: { text: ["STRING"] } },
          },
        });
      else if (request.url.includes("/system_stats"))
        json(response, { devices: [{ type: "cuda" }] });
      else if (request.url.includes("/history/probe"))
        json(response, {
          probe: {
            status: { completed: true, status_str: "success" },
            outputs: {
              save: {
                images: [
                  { filename: "probe.png", subfolder: "", type: "output" },
                ],
              },
            },
          },
        });
      else if (request.url.includes("/view")) {
        response.writeHead(200, { "Content-Type": "image/png" });
        response.end(Buffer.from(original.base64, "base64"));
      } else if (request.url.includes("/history/")) json(response, {});
      else
        json(response, {
          id: mode === "wrong-id" ? "other" : mode,
          status: ["cancelled", "failed"].includes(mode) ? mode : "in_progress",
        });
    },
    async (origin) => {
      const config = configuration(
        root,
        Object.fromEntries(
          modes.map((mode) => [
            mode,
            {
              adapter: "comfyui",
              baseUrl: origin,
              baseUrlMode: "origin",
              auth: { type: "none" },
              defaultModel: "workflow",
              requestTimeoutMs: 700,
              workflow: {
                graph: {
                  text: { class_type: "Text", inputs: { text: "" } },
                  save: { class_type: "Save", inputs: { text: "output" } },
                },
                bindings: { prompt: { node: "text", input: "text" } },
                outputNodes: ["save"],
              },
            },
          ]),
        ),
        context.configSchema,
      );
      for (const mode of modes)
        config.connections[mode].query = { fixture: mode };
      await seed(config, modes, "comfyui", context);
      const store = new context.Store(config.stateDir);
      try {
        assert.equal(store.claimFinalization("completed"), true);
        const done = store.get("completed");
        done.status = "completed";
        done.generation_outcome = "completed";
        store.update(done);
        store.release("completed", done);
      } finally {
        store.close();
      }
      const instances = new Set();
      for (let round = 0; round < 3; round++) {
        const hosts = await pair(root, config, context, instances);
        try {
          for (const mode of modes) {
            const result = await call(
              hosts[0],
              "cancel_job",
              { request_id: mode },
              context.bounded,
            );
            if (!["ambiguous", "wrong-id"].includes(mode)) {
              assert.notEqual(result.isError, true);
              assert.equal(result.structuredContent.refund_verified, false);
              assert.equal(
                result.structuredContent.upstream_cancelled,
                mode === "cancelled"
                  ? true
                  : ["missing", "completed"].includes(mode) ||
                      (mode === "failed" && round > 0)
                    ? null
                    : false,
              );
            } else assert.equal(result.isError, true);
            for (const host of hosts) {
              const current = await job(host, mode, true, context.bounded);
              assert.equal(
                current.status,
                ["cancelled", "failed", "completed"].includes(mode)
                  ? mode
                  : "running",
              );
            }
            const before = generations;
            const probe = await call(
              hosts[1],
              "generate_image",
              {
                connection: mode,
                prompt: "capacity",
                request_id: `capacity-${mode}-${round}`,
              },
              context.bounded,
            );
            if (["cancelled", "failed", "completed"].includes(mode))
              assert.notEqual(probe.isError, true, JSON.stringify(probe));
            assert.equal(
              generations - before,
              ["cancelled", "failed", "completed"].includes(mode) ? 1 : 0,
            );
          }
        } finally {
          await close(hosts, context.closeHost);
        }
        process.stderr.write(
          JSON.stringify({
            component: "native-terminal-acceptance",
            adapter: "comfyui",
            round,
            message: "Cancellation acknowledgement round completed.",
            cases: modes.length,
          }) + "\n",
        );
      }
      assert.equal(instances.size, 6);
      assert.equal(generations, 9);
      assert.equal(cancellations, 2 + 5 * 3);
    },
    context.bounded,
  );
}

async function ownedScenario(context, action) {
  const controller = new AbortController();
  const timer = setTimeout(() => {
    controller.abort(
      new Error("Terminal acceptance exceeded source sixty-second ceiling."),
    );
  }, 60000);
  try {
    // Await the operation itself: cancellation must settle its host/provider finally blocks.
    await action({ ...context, signal: controller.signal });
    controller.signal.throwIfAborted();
  } finally {
    clearTimeout(timer);
  }
}

// Exact three frozen stdio-terminal-lifecycle cases; actual native children, not source dispatch.
export async function terminalAcceptance(context) {
  const images = JSON.parse(
    await readFile(
      join(context.root, ".ci-work/parity/image-acceptance.json"),
      "utf8",
    ),
  );
  const original = images.find((image) => image.name === "png");
  assert.ok(original);
  assert.equal(original.metadata.width, 8);
  assert.equal(original.metadata.height, 6);
  for (const adapter of ["openai-responses", "gemini-interactions"])
    await ownedScenario(context, (scoped) =>
      terminal(adapter, scoped, original),
    );
  await ownedScenario(context, (scoped) => comfy(scoped, original));
}
