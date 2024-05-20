/* eslint-disable react-hooks/exhaustive-deps */

import { ReadSignal, effect } from "@maverick-js/signals"
import { useCallback, useSyncExternalStore } from "react"

type OnChangeCallback<T> = (value: T) => void

export default function useSignalValue<T>(signal: ReadSignal<T>): T {
    const subscribe = useCallback(
        (onChange: OnChangeCallback<T>) => effect(() => onChange(signal())),
        [],
    )
    return useSyncExternalStore(subscribe, signal, signal)
}
