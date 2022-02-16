export default class UsersCounter {
    static bots: {[key: string]: Set<number>} = {};
    static allUsers: Set<number> = new Set();
    static requests = 0;

    static take(userId: number, bot: string) {
        const isExists = this.bots[bot];

        if (!isExists) {
            this.bots[bot] = new Set();
        }

        this.bots[bot].add(userId);
        this.allUsers.add(userId);
        this.requests++;
    }

    static getAllUsersCount(): number {
        return this.allUsers.size;
    }

    static getUsersByBots(): {[bot: string]: number} {
        const result: {[bot: string]: number} = {};

        Object.keys(this.bots).forEach((bot: string) => result[bot] = this.bots[bot].size);

        return result;
    }

    static getMetrics(): string {
        const lines = [];

        lines.push(`all_users_count ${this.getAllUsersCount()}`);
        lines.push(`requests_count ${this.requests}`);

        const usersByBots = this.getUsersByBots();

        Object.keys(usersByBots).forEach((bot: string) => {
            lines.push(`users_count{bot="${bot}"} ${usersByBots[bot]}`)
        });

        return lines.join("\n");
    }
}
