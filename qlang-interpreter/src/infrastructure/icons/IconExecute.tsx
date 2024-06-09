import { IconProps, getIconPropsFrom } from "./utils"

export function IconExecute(props: IconProps) {
    const { className, width, height } = getIconPropsFrom(props)

    return (
        <svg className={`w-6 h-6 text-gray-800 ${className}`} aria-hidden="true" xmlns="http://www.w3.org/2000/svg" width={width} height={height} fill="none" viewBox="0 0 24 24">
            <path stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M16.153 19 21 12l-4.847-7H3l4.848 7L3 19h13.153Z" />
        </svg>
    )
}
