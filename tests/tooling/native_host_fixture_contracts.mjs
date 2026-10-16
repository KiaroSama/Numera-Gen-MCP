import assert from "node:assert/strict";
import { request } from "node:http";
import { test } from "node:test";
import { provider } from "../../scripts/native-host-fixture.mjs";

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
