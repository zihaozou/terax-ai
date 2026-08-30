export { usableActiveSpaceRoot } from "./lib/activeSpace";
export type { SpaceRootFs } from "./lib/rootValidation";
export { validateSpaceRoot } from "./lib/rootValidation";
export type {
  PreparedWorkspace,
  SpaceController,
  SpaceControllerDeps,
} from "./lib/spaceController";
export {
  createSpaceController,
  nextSpaceName,
} from "./lib/spaceController";
export type { SpaceMeta } from "./lib/store";
export { useSpacePersistence } from "./lib/useSpacePersistence";
export {
  canPersistSpaceState,
  useSpaces,
} from "./lib/useSpaces";
export { useSpacesBoot } from "./lib/useSpacesBoot";
export { SpaceAvatar } from "./SpaceAvatar";
export { SpaceSwitcher } from "./SpaceSwitcher";
