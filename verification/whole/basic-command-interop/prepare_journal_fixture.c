#include <sqlite3.h>
#include <stdio.h>
#include <string.h>

extern int sqlite3_key(sqlite3 *, const void *, int);

int main(int argc, char **argv) {
    static const char *queries[] = {
        "SELECT local_account,remote_account,relay,session,hex(native_group),ready "
        "FROM local_room_prepare_v1 ORDER BY session LIMIT 2",
        "SELECT hex(session),hex(peer),hex(group_id),direction,revoked "
        "FROM command_rooms ORDER BY session LIMIT 2",
    };
    if (argc != 3 || (strcmp(argv[1], "placement") && strcmp(argv[1], "admission"))) return 1;
    const int kind = !strcmp(argv[1], "admission");
    char key[67];
    if (fread(key, 1, sizeof(key), stdin) != sizeof(key) || fgetc(stdin) != EOF) return 1;
    sqlite3 *db = NULL;
    int result = sqlite3_open_v2(argv[2], &db, SQLITE_OPEN_READONLY, NULL);
    if (result == SQLITE_OK) result = sqlite3_key(db, key, sizeof(key));
    volatile char *clear = key;
    for (size_t position = 0; position < sizeof(key); ++position) clear[position] = 0;
    sqlite3_stmt *statement = NULL;
    if (result == SQLITE_OK) result = sqlite3_busy_timeout(db, 1500);
    if (result == SQLITE_OK) result = sqlite3_prepare_v2(db, queries[kind], -1, &statement, NULL);
    if (result == SQLITE_OK) {
        while ((result = sqlite3_step(statement)) == SQLITE_ROW) {
            for (int column = 0; column < sqlite3_column_count(statement); ++column) {
                const unsigned char *value = sqlite3_column_text(statement, column);
                printf("%s%s", column ? " " : "", value ? (const char *) value : "-");
            }
            putchar('\n');
        }
    }
    sqlite3_finalize(statement);
    sqlite3_close(db);
    return result == SQLITE_DONE ? 0 : 2;
}
