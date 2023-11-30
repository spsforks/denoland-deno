// Copyright 2018-2023 the Deno authors. All rights reserved. MIT license.

// ** Internal Interfaces **

interface PendingAbortRequest {
  deferred: Deferred<void>;
  // deno-lint-ignore no-explicit-any
  reason: any;
  wasAlreadyErroring: boolean;
}

// deno-lint-ignore no-explicit-any
interface ReadRequest<R = any> {
  chunkSteps: (chunk: R) => void;
  closeSteps: () => void;
  // deno-lint-ignore no-explicit-any
  errorSteps: (error: any) => void;
}

interface ReadIntoRequest {
  chunkSteps: (chunk: ArrayBufferView) => void;
  closeSteps: (chunk?: ArrayBufferView) => void;
  // deno-lint-ignore no-explicit-any
  errorSteps: (error: any) => void;
}

interface PullIntoDescriptor {
  buffer: ArrayBuffer;
  bufferByteLength: number;
  byteOffset: number;
  byteLength: number;
  bytesFilled: number;
<<<<<<< HEAD
  minimumFill: number;
=======
>>>>>>> 8c07f52a7 (1.38.4 (#21398))
  elementSize: number;
  // deno-lint-ignore no-explicit-any
  viewConstructor: any;
  readerType: "default" | "byob" | "none";
}

interface ReadableByteStreamQueueEntry {
  buffer: ArrayBufferLike;
  byteOffset: number;
  byteLength: number;
}

interface ReadableStreamGetReaderOptions {
  mode?: "byob";
}

interface ReadableStreamIteratorOptions {
  preventCancel?: boolean;
}

interface ValueWithSize<T> {
  value: T;
  size: number;
}

interface VoidFunction {
  (): void;
}

interface ReadableStreamGenericReader<T> {
  readonly closed: Promise<void>;
  // deno-lint-ignore no-explicit-any
  cancel(reason?: any): Promise<void>;
}

// ** Ambient Definitions and Interfaces not provided by fetch **

declare function queueMicrotask(callback: VoidFunction): void;

declare namespace Deno {
  function inspect(value: unknown, options?: Record<string, unknown>): string;
}
