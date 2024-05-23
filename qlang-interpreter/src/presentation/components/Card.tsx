interface TileProps {
    title: string | React.ReactNode;
    children: React.ReactNode;
}

export default function Card({ title, children }: TileProps) {
    return (
        <div className="shadow-md dark:shadow-ld-zinc-600 rounded-xl w-full flex flex-col justify-stretch content-stretch">
            <h2 className="rounded-t-xl py-1 px-2 font-bold">
                {title}
            </h2>
            <div className="rounded-b-xl flex-1 py-1 px-2">
                {children}
            </div>
        </div>
    )
}
