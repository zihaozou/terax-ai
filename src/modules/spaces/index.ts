export type { DirectoryPickerMode } from "./components/SpaceDirectoryPicker";
export { SpaceDirectoryPicker } from "./components/SpaceDirectoryPicker";
export type { SpaceRootFs } from "./lib/rootValidation";
export { validateSpaceRoot } from "./lib/rootValidation";
export type {
  PreparedWorkspace,
  SpaceController,
  SpaceControllerDeps,
} from "./lib/spaceController";
export { createSpaceController } from "./lib/spaceController";
export type { SpaceMeta } from "./lib/store";
export { useSpacePersistence } from "./lib/useSpacePersistence";
export { useSpaces } from "./lib/useSpaces";
export { useSpacesBoot } from "./lib/useSpacesBoot";
export { SpaceAvatar } from "./SpaceAvatar";
export { SpaceRootRecovery } from "./components/SpaceRootRecovery";
export { SpaceSwitcher } from "./SpaceSwitcher";
