interface TileProps {
    title: string | React.ReactNode;
    children: React.ReactNode;
}

export default function Tile({ title, children }: TileProps) {
    return (
        <div className="shadow-md dark:shadow-ld-zinc-600 rounded w-full h-full flex flex-col justify-stretch content-stretch">
            <h2 className="rounded-t py-1 px-2 font-bold bg-zinc-50 dark:bg-zinc-900">
                {title}
            </h2>
            <div className="rounded-b flex-1 overflow-auto flex justify-stretch content-stretch">
                {children}
            </div>
        </div>
    )
}
