/* tslint:disable */
/* eslint-disable */

/**
 * Worker-owned runtime facade. No Document mutator crosses this boundary.
 */
export class EngineHost {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Accepts one versioned request and returns one versioned, correlated JSON response.
     */
    handleJson(request_json: string): string;
    constructor();
    static protocolVersion(): number;
    static renderBinarySchemaVersion(): number;
    takeDirtyInstances(): Uint8Array;
    takeFullInstances(): Uint8Array;
    takeRemovedSlots(): Uint32Array;
    takeVisibleSlots(): Uint32Array;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_enginehost_free: (a: number, b: number) => void;
    readonly enginehost_new: () => [number, number, number];
    readonly enginehost_protocolVersion: () => number;
    readonly enginehost_renderBinarySchemaVersion: () => number;
    readonly enginehost_handleJson: (a: number, b: number, c: number) => [number, number];
    readonly enginehost_takeFullInstances: (a: number) => any;
    readonly enginehost_takeDirtyInstances: (a: number) => any;
    readonly enginehost_takeRemovedSlots: (a: number) => any;
    readonly enginehost_takeVisibleSlots: (a: number) => any;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
