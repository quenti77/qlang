/* eslint-disable @typescript-eslint/no-explicit-any */
import { useRef, useState } from "react"

export default function useReactive<T>(initialState: T) {
    const [, setTick] = useState(-1)
    const stateRef = useRef<T>(initialState)

    const setRef = (value: T) => {
        stateRef.current = value

        const reactiveHandler = {
            get(target: any, prop: any, receiver: any) {
                return Reflect.get(target, prop, receiver)
            },
            set(target: any, prop: any, value: any, receiver: any) {
                const result = Reflect.set(target, prop, value, receiver)
                setTick(tick => tick + 0)
                return result
            }
        }

        const proxyState = new Proxy(stateRef.current, reactiveHandler)
        stateRef.current = proxyState // Update ref to proxy

        setTick(tick => tick + 0)
    }

    return stateRef
        ? [stateRef.current, setRef] as [T, (value: T) => void]
        : [null, setRef] as [T, (value: T) => void]
}
