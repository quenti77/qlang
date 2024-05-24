const sizes = {
    xs: { width: 12, height: 12 },
    sm: { width: 16, height: 16 },
    md: { width: 24, height: 24 },
    lg: { width: 32, height: 32 },
    xl: { width: 48, height: 48 },
}

export interface IconProps {
    className?: string
    size: keyof typeof sizes
}

export function getIconPropsFrom({ className = '', size }: IconProps) {
    const { width, height } = sizes[size]
    return { className, width, height }
}
