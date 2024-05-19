import CodeTile from "@/application/home/components/CodeTile"
import HelpTile from "@/application/home/components/HelpTile"
import OutputTile from "@/application/home/components/OutputTile"

export default function Home() {
    return (
        <div className="m-2 grid grid-rows-3 grid-cols-4 gap-2 h-[calc(100vh-4rem)]">
            <div className="hidden lg:block lg:row-span-2 col-span-2 xl:col-span-1">
                <HelpTile />
            </div>
            <div className="row-span-2 col-span-4 lg:col-span-2 xl:col-span-3">
                <CodeTile />
            </div>
            <div className="row-start-3 row-span-1 col-span-4">
                <OutputTile />
            </div>
        </div>
    )
}
