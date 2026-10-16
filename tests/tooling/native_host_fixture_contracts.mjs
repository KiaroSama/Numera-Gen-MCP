import assert from "node:assert/strict";
import { request } from "node:http";
import { test } from "node:test";
import { provider } from "../../scripts/native-host-fixture.mjs";
import { probeStages } from "../../scripts/native-terminal-acceptance.mjs";

test("probe diagnostics expose only fixed stages and numeric durations", () => {
  const before = "Earlier capture\n";
  const privateText = "synthetic-private";
  const rows = [
    {
      component: "tool",
      tool: "generate_image",
      message: "Tool started.",
      call_id: privateText,
    },
    {
      component: "generation",
      message: "Submission intent persisted.",
      requestId: privateText,
    },
    {
      component: "http",
      message: "Submission transport prepared.",
      client_build_ms: 900,
      pre_send_ms: 901,
      timeout_ms: 700,
      url: privateText,
      authorization: privateText,
    },
    {
      component: "http",
      message: "Submission transport prepared.",
      client_build_ms: -1,
      pre_send_ms: Infinity,
      timeout_ms: privateText,
    },
    {
      component: "tool",
      tool: "generate_image",
      message: "Tool completed.",
      duration_ms: 701,
      prompt: privateText,
    },
    {
      component: "tool",
      tool: "generate_image",
      message: "Tool failed.",
      duration_ms: privateText,
    },
    { component: "tool", tool: "generate_image", message: "constructor" },
    { component: "tool", tool: "get_job", message: "Tool started." },
    null,
  ];
  const after =
    before + rows.map((row) => JSON.stringify(row)).join("\n") + "\n";
  const projected = probeStages(before, after + '{"unfinished":');
  assert.deepEqual(projected, {
    capture: "available_rows_only",
    stages: [
      { stage: "started" },
      { stage: "intent" },
      {
        stage: "transport_prepared",
        client_build_ms: 900,
        pre_send_ms: 901,
        timeout_ms: 700,
      },
      { stage: "transport_prepared" },
      { stage: "completed", duration_ms: 701 },
      { stage: "failed" },
    ],
  });
  assert.equal(JSON.stringify(projected).includes(privateText), false);
  assert.deepEqual(probeStages(before, after.slice(before.length)), {
    capture: "unobserved",
    stages: [],
  });
});

async function bounded(promise, ms = 1000) {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(
          () => reject(new Error("Owned helper contract timed out.")),
          ms,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

for (const runFails of [false, true]) {
  test(
    `late provider-handler error fails cleanup with run failure=${runFails}`,
    { timeout: 5000 },
    async () => {
      const late = new Error("Controlled late handler failure");
      const original = new Error("Controlled original scenario failure");
      let enter;
      const entered = new Promise((accept) => {
        enter = accept;
      });
      let client;
      let clientClosed;
      await assert.rejects(
        provider(
          async (_incoming, response) => {
            const closed = new Promise((accept) =>
              response.once("close", accept),
            );
            enter();
            await bounded(closed);
            throw late;
          },
          async (origin) => {
            client = request(origin);
            client.on("error", () => {});
            clientClosed = new Promise((accept) =>
              client.once("close", accept),
            );
            client.end();
            await bounded(entered);
            if (runFails) throw original;
            return "Scenario body passed before late failure";
          },
          bounded,
        ),
        (error) => {
          assert.ok(error instanceof AggregateError);
          assert.ok(error.errors.includes(late));
          if (runFails) assert.equal(error.errors[0], original);
          return true;
        },
      );
      await bounded(clientClosed);
      assert.equal(client.destroyed, true);
    },
  );
}
