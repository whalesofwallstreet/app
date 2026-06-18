export default {
    base: "/app/",

    title: "Whales of Wallstreet",
    description: "WOW Monorepo Documentation",

    themeConfig: {
        nav: [
            { text: "Overview", link: "/" },
            { text: "Engine", link: "/engine/" },
            { text: "Apps", link: "/apps/" }
        ],

        sidebar: {
            "/engine/": [
                {
                    text: "Wow Engine",
                    items: [
                        { text: "Overview", link: "/engine/" }
                    ]
                }
            ],

            "/apps/": [
                {
                    text: "Applications",
                    items: [
                        { text: "Overview", link: "/apps/" }
                    ]
                }
            ]
        }
    }
}