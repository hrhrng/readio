import {
  createReadioSdkClient,
  type ReadioSdkWasmModule,
} from '@readio/sdk'
import init, {
  type InitInput,
} from '../../../web/wasm/readio_wasm.js'
import * as generatedWasm from '../../../web/wasm/readio_wasm.js'

export function createExtensionReadioSdkLoader(input?: InitInput) {
  let pending: Promise<ReadioSdkWasmModule> | undefined

  return async (): Promise<ReadioSdkWasmModule> => {
    pending ??= init(
      input === undefined ? undefined : { module_or_path: input },
    ).then(() => generatedWasm as unknown as ReadioSdkWasmModule)
    return pending
  }
}

export const loadExtensionReadioWasm = createExtensionReadioSdkLoader()
export const extensionReadioSdk = createReadioSdkClient(
  loadExtensionReadioWasm,
)
