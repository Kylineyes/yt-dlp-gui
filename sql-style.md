# SQLite SQL 书写规范

本规范适用于仓库中所有 SQLite schema、查询、写入、迁移和测试 SQL。SQL 通过 Rust 的 `rusqlite` 调用时，也必须遵守本规范。

## 基本规则

- SQL 关键字、表名和列名一律使用小写。
- 表名、列名、索引名和约束名使用 `snake_case`。
- 每条 SQL 语句都按逻辑子句分行书写；不得为简短语句改写为单行。
- Rust 源文件仍须符合 `rustfmt.toml` 的 120 列限制；SQL 的分行优先保证语义清晰，不得依赖超长字符串绕过行宽检查。
- SQL 字符串应放在其使用位置附近；同一查询在多个调用点复用时，使用具名常量表达其含义。

## 查询

- `select` 的每个列名独占一行，并缩进四个空格。
- `from`、`join`、`where`、`order by`、`limit` 等子句分别独占一行。
- 条件表达式缩进四个空格；多个条件使用 `and` 或 `or` 起行。
- 显式列出所需列，不使用 `select *`。
- 查询结果顺序影响界面、任务恢复或业务逻辑时，必须显式声明 `order by`。

```sql
select
    id,
    status,
    updated_at
from
    download_tasks
where
    status = ?1
order by
    updated_at desc,
    id desc
limit
    ?2;
```

## 写入

- `insert into` 后的列清单逐列换行。
- `values` 的占位符或常量值与列清单保持同一顺序，并逐项换行。
- `update` 的 `set` 子句逐列换行；`where` 不能省略，除非语句意图操作整张表且已在事务注释中说明。
- `delete` 必须有明确的 `where` 条件；清空受单记录约束的配置表是唯一例外，且必须在受控事务中执行。

```sql
update
    download_tasks
set
    status = ?1,
    updated_at = ?2
where
    id = ?3;
```

## 占位符与动态条件

- 所有外部输入和运行时值使用 `?n` 占位符绑定；禁止通过字符串拼接插入值。
- 占位符从 `?1` 开始，按 SQL 中首次出现的顺序连续编号。
- 仅允许拼接由内部固定 SQL 片段组成的动态子句；不得拼接来自外部输入的表名、列名、运算符或值。
- 动态 SQL 的每个可选片段必须保持参数编号与绑定参数一致，并由测试覆盖。

## 事务与配置保存

- 多表写入、状态迁移和需要同时更新多条记录的操作必须使用 SQLite 事务。
- 事务失败时不得更新内存快照；仅在事务提交成功后更新内存状态。
- `config` 表只保存一条完整环境配置记录，必须通过单行 UPSERT 原子保存。
- `config` 表必须定义稳定的单记录约束，UPSERT 使用该约束作为冲突目标；禁止通过“先删除再插入”实现配置保存。
- 首次创建 `config` 表时不写入配置记录；首次完整配置保存由 `feature/storage` 的 `save_configuration` 接口执行。

```sql
insert into
    config (
        singleton,
        version,
        theme
    )
values (
    1,
    ?1,
    ?2
)
on conflict (singleton) do update set
    version = excluded.version,
    theme = excluded.theme;
```

## Schema 与迁移

- `create table`、`create index`、`alter table` 和 `pragma` 也遵守小写及分行规则。
- 所有表必须声明主键或可解释的唯一约束；关联字段必须有外键或在规范中说明不使用外键的原因。
- 需要按状态、时间或关联标识检索的数据，应建立与查询顺序匹配的索引。
- Schema 变更必须同时更新版本记录、迁移逻辑和独立测试。
- SQLite 连接建立后必须启用并验证 `pragma foreign_keys = on`。

## 测试 SQL

- 测试中的 SQL 与生产 SQL 使用同一规范，不因测试而放宽分行、命名、参数化或事务规则。
- 每项 schema、约束、级联删除、UPSERT、状态迁移和动态查询规则都应由独立测试覆盖。
