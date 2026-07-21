import type { MyOpenPanelsBuildInfo } from "../types"

export type MyOpenPanelsChannel = MyOpenPanelsBuildInfo["channel"]
export type AgentRuntimeIdentity = Pick<
  MyOpenPanelsBuildInfo,
  "agentCli" | "channel"
>

export function agentCliExecutable(runtime: AgentRuntimeIdentity): string {
  const executable =
    runtime.agentCli?.trim() ||
    (runtime.channel === "development"
      ? "scripts/myopenpanels-dev"
      : "myopenpanels")
  if (/^[A-Za-z0-9_./:-]+$/.test(executable)) return executable
  return `'${executable.replaceAll("'", "'\\''")}'`
}

export function agentCliBoundaryInstruction(
  runtime: AgentRuntimeIdentity,
  locale: "en" | "zh-CN"
): string {
  if (locale === "zh-CN") {
    return runtime.channel === "development"
      ? "当前 Studio 为开发版。只使用仓库内的 scripts/myopenpanels-dev，不要运行已安装的正式版 myopenpanels。"
      : "当前 Studio 为正式版。只使用已安装的 myopenpanels，不要运行仓库内的 scripts/myopenpanels-dev。"
  }
  return runtime.channel === "development"
    ? "This Studio is a development build. Use only the checkout-local scripts/myopenpanels-dev and do not run the installed release myopenpanels."
    : "This Studio is a release build. Use only the installed myopenpanels and do not run the checkout-local scripts/myopenpanels-dev."
}

export function agentRecoveryInstruction(
  runtime: AgentRuntimeIdentity
): string {
  const cli = agentCliExecutable(runtime)
  if (runtime.channel === "development") {
    return `当前 Studio 为开发版。请在 MyOpenPanels 源码仓库根目录运行 corepack pnpm --dir apps/studio build；然后运行 ${cli} studio stop --project-dir "$PWD" --format json；最后运行 ${cli} studio start --local-only --project-dir "$PWD" --format json。不要运行 myopenpanels update install，它只适用于正式版。`
  }
  return `当前 Studio 为正式版。请先运行 ${cli} update install --format json 安装最新的 MyOpenPanels CLI；安装成功后，再运行 ${cli} studio start --local-only --project-dir "$PWD" --format json 重新启动 Studio。不要运行 scripts/myopenpanels-dev。`
}
