import { expect, test } from "vitest";
import { instanceRowKey, leave, serverRowKey, takeReturn } from "./focus";
import { view } from "./operation";

test("going back remembers the row once and shows the list", () => {
  expect(takeReturn()).toBeNull();
  leave(instanceRowKey("atm10"), "instances");
  expect(view.value).toBe("instances");
  expect(takeReturn()).toBe("instance:atm10");
  expect(takeReturn()).toBeNull();
  leave(serverRowKey("panel-a", "home"), "servers");
  expect(takeReturn()).toBe("server:panel-a/home");
});
