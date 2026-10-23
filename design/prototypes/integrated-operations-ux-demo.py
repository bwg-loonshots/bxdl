#!/usr/bin/env python3
"""Terminal interaction prototype. No file, network, process or service operations."""

import argparse
from dataclasses import dataclass, field
import sys


class ExitDemo(Exception):
    pass


@dataclass
class Draft:
    name: str = "새 validator"
    node_id: str = "validator-a"
    http_port: int = 18080
    p2p_port: int = 19090
    materials: set = field(default_factory=set)
    stage: str = "materials"
    node: object = None


@dataclass
class Node:
    name: str
    state: str
    node_id: str = "validator-a"
    http_port: int = 18080
    p2p_port: int = 19090
    scenario: str = "normal"
    observed: bool = False
    attempt: int = 1
    last: str = "READY · 이전 관측 예시"


STATES = {
    "registered": "설치·등록 완료 · 초기화 전",
    "initialized": "초기화 완료 · 시작 전",
    "starting": "시작 요청됨 · 준비 상태 확인 필요",
    "timeout": "관측 시간이 끝남 · 계속 시작 중일 수 있음",
    "ready": "실행 중 · 로컬 준비 완료",
    "stopping": "중지 요청됨 · 정상 종료 확인 필요",
    "unverified": "정지가 관측됨 · 정상 종료 확인 필요",
    "stopped": "정상 종료 확인됨",
    "unknown": "이전 실행 결과 불명 · 지원 검토 필요",
}

SCENARIOS = {
    "ready": ("운용 중인 노드", "ready"),
    "timeout": ("시작 관측 시간 만료", "timeout"),
    "unverified": ("정지 관측 · 종료 미확정", "unverified"),
    "unknown": ("이전 실행 결과 불명", "unknown"),
    "mismatch": ("설치 CLI 불일치", "ready"),
    "changed": ("승인 중 실행 대상 변경", "ready"),
}


class Demo:
    def __init__(self):
        self.nodes = [Node("운영 예시 validator-b", "ready", node_id="validator-b")]
        self.drafts = [Draft(name="이어하기 예시", materials={1, 2, 3}, stage="settings")]

    @staticmethod
    def say(message=""):
        print(message)

    def screen(self, title):
        self.say("\n" + "─" * 46)
        self.say("BXDL 콘솔 체험 · 모든 상태와 결과는 예시")
        self.say(title)
        self.say("─" * 46)

    def read(self, prompt, help_text):
        while True:
            try:
                value = input(prompt).strip()
            except (EOFError, KeyboardInterrupt):
                self.say()
                raise ExitDemo
            if value == ":help":
                self.say("\n" + help_text)
                self.say(":back 이전 화면 · :cancel 데모 종료 · 승인 Enter=거절")
                continue
            if value == ":cancel":
                raise ExitDemo
            if len(value) > 128 or any(not ch.isprintable() for ch in value):
                self.say("128자 이내의 표시 가능한 문자를 입력하세요.")
                continue
            return value

    def confirm(self, description):
        self.say("\n" + description)
        value = self.read("이 작업을 체험할까요? [y/N] > ",
                          "y 또는 yes만 이 화면의 한 작업을 승인합니다. 다른 작업은 별도 확인합니다.")
        if value.lower() in {"y", "yes"}:
            return True
        self.say("승인하지 않았습니다. 예시 상태도 변경하지 않았습니다.")
        return False

    def pause(self, help_text="Enter 또는 :back으로 이전 화면에 돌아갑니다."):
        self.read("돌아가기 [Enter] > ", help_text)

    def run(self, scenario=None):
        self.screen("설치·운용 UX 미리 체험하기")
        self.say("제품의 bxdl console은 아직 구현 중입니다.")
        self.say("이 프로그램은 파일·네트워크·실제 명령·서비스를 조작하지 않습니다.")
        self.say("예시 자료만 사용합니다. 실제 경로·키·비밀번호를 입력하지 마세요.")
        self.say("종료하면 모든 체험 상태가 사라집니다. 이어하기는 실행 중에만 됩니다.")
        self.say("번호 선택 / :help 도움말 / :back 이전 / :cancel 종료 / Ctrl-C 종료")
        if scenario:
            self.load_scenario(scenario)
        while True:
            self.screen("홈 · Mac 사용자 노드")
            self.say("1  새 노드 설치 체험\n2  중단한 설치 이어하기\n3  내 노드 관리")
            self.say("4  도움말과 준비물\n9  문제 상황 골라보기 [데모 전용]\n0  종료")
            choice = self.read("선택 [0] > ",
                               "처음에는 1번으로 설치, 빠른 운용 체험은 3번입니다. 9번은 예시 상태를 추가합니다.")
            if choice in {"", "0", ":back"}:
                return
            if choice == "1":
                draft = Draft()
                self.drafts.append(draft)
                self.install(draft)
            elif choice == "2":
                self.resume()
            elif choice == "3":
                self.node_list()
            elif choice == "4":
                self.help()
            elif choice == "9":
                self.scenario_menu()
            else:
                self.say("표시된 번호를 선택하세요.")

    def help(self):
        self.screen("준비물과 담당자")
        self.say("제품 공급 담당자: package, engine lock, 독립 경로로 확인한 신뢰키")
        self.say("네트워크 관리자: 승인 chain·공개 노드 목록·발급된 자격 자료")
        self.say("설치 담당자: 내 노드 식별, 주소·포트·저장 위치 검토, 단계별 승인")
        self.say("\n이 데모에서는 각 자료 선택에 예시가 자동 입력됩니다.")
        self.say("실제 제품에서는 파일 선택·신뢰/일치 검사·권한 검사가 필요합니다.")
        self.say("설치 → 초기화 → 시작은 각각 승인합니다. Enter는 실행하지 않습니다.")
        self.say("\nMac LaunchAgent: 터미널 종료로 노드를 중지하지 않습니다.")
        self.say("Mac 사용자 로그아웃 시 종료되며 다음 로그인 때 자동 시작하지 않습니다.")
        self.say("로컬 READY는 네트워크 정족수·합의 확인과 다릅니다.")
        self.say("\n이 체험 도구는 Python 표준 라이브러리만 사용합니다.")
        self.say("제품 CLI는 계속 Rust로 구현하며, 이 데모 코드를 제품에 연결하지 않습니다.")
        self.pause()

    def install(self, draft):
        while True:
            if draft.node is not None:
                self.say("저장 결과 재검증 화면 예시입니다. 실제 파일은 검사하지 않습니다.")
                self.operate(draft.node)
                return
            if draft.stage == "materials":
                if not self.materials(draft):
                    return
            elif draft.stage == "settings":
                if not self.settings(draft):
                    return
            elif draft.stage == "plan":
                if not self.plan(draft):
                    return

    def materials(self, draft):
        roles = {
            1: "제품 공급 자료 · package/lock/독립 확인 신뢰키",
            2: "승인 네트워크 자료 · chain/공개 노드 목록",
            3: "자격 자료 · 키/인증서/비밀번호 파일 참조",
        }
        while True:
            self.screen("새 노드 설치 · 1/4 준비자료")
            for number, label in roles.items():
                status = "예시 선택됨 · 검사 전" if number in draft.materials else "미제공"
                self.say(f"{number}  {label}\n   {status}")
            self.say("4  준비자료 검사 후 다음\n5  필요한 자료와 담당 보기\n0  초안 남기고 홈으로")
            choice = self.read("선택 [0] > ",
                               "1~3은 실제 파일 대신 예시를 선택합니다. 4번 검사가 끝나야 다음으로 갑니다.")
            if choice in {"", "0", ":back"}:
                self.say("초안을 이번 실행의 메모리에 남겼습니다. 홈 2번에서 이어갈 수 있습니다.")
                return False
            if choice in {"1", "2", "3"}:
                draft.materials.add(int(choice))
                self.say("예시 자료를 선택했습니다. 파일을 읽지 않았습니다.")
            elif choice == "4":
                missing = {1, 2, 3} - draft.materials
                if missing:
                    self.say("설치를 시작하지 않았습니다. 필요한 자료: " + ", ".join(str(i) for i in sorted(missing)))
                else:
                    self.say("[예시 검사] 자료 일치·신뢰 확인 완료. 실행 검사는 아직 안 했습니다.")
                    draft.stage = "settings"
                    return True
            elif choice == "5":
                self.help()
            else:
                self.say("표시된 번호를 선택하세요.")

    def settings(self, draft):
        while True:
            self.screen("새 노드 설치 · 2/4 설정 검토")
            self.say(f"1  표시 이름       {draft.name}\n2  승인 목록 내 노드 {draft.node_id}")
            self.say(f"3  노드 HTTP       127.0.0.1:{draft.http_port}")
            self.say(f"4  P2P             192.0.2.10:{draft.p2p_port}")
            self.say("5  고급 설정·자료 보존 설명")
            self.say("   참여 방식       승인된 기존 네트워크에 참여")
            self.say("   저장 위치       BXDL 기본 관리 위치 [예시]")
            self.say("   자격 자료       역할별 선택됨 [예시] · 내용 표시 안 함")
            self.say("9  설치 계획 검토 (review)\n0  초안 남기고 홈으로")
            choice = self.read("수정할 번호 / review / :back [0] > ",
                               "한 항목을 고치면 이 검토표로 돌아옵니다. :back은 준비자료 화면으로 돌아갑니다.")
            if choice in {"", "0"}:
                return False
            if choice == ":back":
                draft.stage = "materials"
                return True
            if choice in {"9", "review"}:
                draft.stage = "plan"
                return True
            if choice in {"1", "2", "3", "4"}:
                self.edit(draft, choice)
            elif choice == "5":
                self.screen("고급 설정 · 예시 값")
                self.say("TLS: 발급된 자료 참조 / archive: 승인 정책 유지 / consensus: QBFT")
                self.say("기본 위치: ~/Library/Application Support/BXDL [가상 경로]")
                self.say("실제 제품은 새 package/control/data/session 경로를 검사합니다.")
                self.say("제품/native/chain/키 참조·완료된 설치 session은 계속 보존합니다.")
                self.say("이 데모에서는 고급 설정 편집과 경로·자료 검사를 생략합니다.")
                self.pause()
            else:
                self.say("표시된 번호 또는 review를 입력하세요.")

    def edit(self, draft, choice):
        prompts = {"1": "새 표시 이름", "2": "예시 승인 노드 번호 (1=a, 2=b, 3=c, 4=d)",
                   "3": "새 노드 HTTP 포트", "4": "새 P2P 포트"}
        while True:
            value = self.read(prompts[choice] + " [Enter=유지] > ",
                              "공개 설정 하나만 수정합니다. Enter 또는 :back은 현재 값을 유지합니다.")
            if value in {"", ":back"}:
                return
            if choice == "1" and len(value) <= 48:
                draft.name = value
            elif choice == "2" and value in {"1", "2", "3", "4"}:
                draft.node_id = "validator-" + "abcd"[int(value) - 1]
                self.say("[예시] 해당 노드의 자격 자료 일치 검사를 함께 통과한 상황입니다.")
            elif choice in {"3", "4"} and value.isascii() and value.isdigit() and 1024 <= int(value) <= 65535:
                other = draft.p2p_port if choice == "3" else draft.http_port
                if int(value) == other:
                    self.say("HTTP와 P2P에 서로 다른 포트를 선택하세요.")
                    continue
                if choice == "3":
                    draft.http_port = int(value)
                else:
                    draft.p2p_port = int(value)
            else:
                self.say("이름은 48자 이내, 노드 번호는 1~4, 예시 포트는 1024~65535를 사용하세요.")
                continue
            self.say("항목을 반영했습니다. 설치 전에 새 계획을 검토합니다.")
            return

    def plan(self, draft):
        self.screen("새 노드 설치 · 3/4 설치 계획")
        self.say(f"대상             {draft.name} / {draft.node_id}")
        self.say("제품             새 BXDL console 포함 package [미출시·예시]")
        self.say("엔진 / runtime   NIGO RC.3 / 선정 Temurin 21 [예시]")
        self.say(f"노드 HTTP / P2P  {draft.http_port} / {draft.p2p_port}")
        self.say("생성             기본 관리 위치 내 새 package/config/control/session")
        self.say("보존             제품/native/chain/키 참조·설치 session")
        self.say("DB / 서비스      초기화 전 / 시작 전")
        self.say("실제 경로·서명·저장 공간·잠금 검사는 데모에서 수행하지 않습니다.")
        self.say("1  설치·등록 확인\n2  설정 수정\n0  초안 남기고 홈으로")
        choice = self.read("선택 [0] > ", "1번 뒤에도 별도 y 승인이 있어야 설치 결과 예시로 이동합니다.")
        if choice in {"", "0"}:
            return False
        if choice in {"2", ":back"}:
            draft.stage = "settings"
        elif choice == "1":
            if self.confirm(f"{draft.name}: 새 설치·등록만 진행합니다. DB 초기화와 시작은 별도입니다."):
                node = Node(draft.name, "registered", node_id=draft.node_id,
                            http_port=draft.http_port, p2p_port=draft.p2p_port,
                            observed=True, attempt=0, last="설치·등록 완료 [예시]")
                draft.node = node
                draft.stage = "installed"
                self.nodes.append(node)
                self.say("[예시 결과] 설치·등록 완료. 파일을 만들지 않았습니다.")
        else:
            self.say("표시된 번호를 선택하세요.")
        return True

    def resume(self):
        while True:
            self.screen("중단한 설치 이어하기 · 이번 데모 실행 안에서만 유지")
            for i, draft in enumerate(self.drafts, 1):
                status = "설치됨 · 노드 관리로" if draft.node else {
                    "materials": "준비자료 선택 중", "settings": "설정 검토 중", "plan": "설치 승인 전"
                }[draft.stage]
                self.say(f"{i}  {draft.name} · {status}")
            self.say("0  홈으로")
            choice = self.read("선택 [0] > ",
                               "제품은 저장 결과와 입력을 재검증합니다. 데모는 메모리의 예시 상태만 다시 엽니다.")
            if choice in {"", "0", ":back"}:
                return
            if choice.isascii() and choice.isdigit() and 1 <= int(choice) <= len(self.drafts):
                draft = self.drafts[int(choice) - 1]
                if draft.node is not None:
                    draft.node.observed = False
                self.install(draft)
            else:
                self.say("목록의 번호를 선택하세요.")

    def node_list(self):
        while True:
            self.screen("내 노드 · 목록은 마지막 기록이며 현재 상태는 미조회")
            for i, node in enumerate(self.nodes, 1):
                self.say(f"{i}  {node.name}\n   마지막 예시 기록: {node.last}")
            self.say("0  홈으로")
            choice = self.read("선택 [0] > ",
                               "목록을 여는 것만으로 현재 상태를 조회하지 않습니다. 노드를 고른 뒤 1번으로 확인하세요.")
            if choice in {"", "0", ":back"}:
                return
            if choice.isascii() and choice.isdigit() and 1 <= int(choice) <= len(self.nodes):
                node = self.nodes[int(choice) - 1]
                node.observed = False
                self.operate(node)
            else:
                self.say("목록의 번호를 선택하세요.")

    @staticmethod
    def action(node):
        if node.scenario == "mismatch":
            return None, "변경 불가 · 설치 CLI 확인 필요"
        if not node.observed:
            return None, "상태를 먼저 새로 확인하세요"
        if node.state == "registered":
            return "init", "DB 초기화 확인"
        if node.state in {"initialized", "stopped"}:
            return "start", "같은 데이터로 시작 확인" if node.state == "stopped" else "노드 시작 확인"
        if node.state in {"ready", "starting", "timeout", "unverified"}:
            return "stop", "정상 종료 요청·확인"
        return None, "정상 종료 확인 중" if node.state == "stopping" else "변경 불가 · 지원 검토 필요"

    def operate(self, node):
        while True:
            self.screen(f"노드 관리 · {node.name}")
            if node.observed:
                self.say("상태 [예시 관측]  " + STATES[node.state])
            else:
                self.say("현재 상태         미조회 · 1번으로 새로 확인")
                self.say("마지막 기록       " + node.last)
            self.say(f"노드 / 실행 시도  {node.node_id} / demo-attempt-{node.attempt}")
            self.say("합의·정족수       미확인")
            if node.scenario == "mismatch":
                self.say("선택한 설치와 이 CLI의 바이트가 다릅니다. 변경 작업을 차단합니다.")
            if node.observed and node.state == "timeout":
                self.say("실패로 확정하지 않습니다. 같은 시도 확인만으로 다음 상태를 관측합니다.")
            if node.observed and node.state == "unknown":
                self.say("자동 복구·재초기화·잠금 삭제를 하지 않습니다. 정제 보고서로 지원을 요청하세요.")
            action, label = self.action(node)
            self.say("1  " + ("같은 실행 시도 다시 확인" if node.state in {"starting", "timeout"} else "상태 새로 확인"))
            self.say("2  " + label + "\n3  최근 정제 기록\n4  지원 보고서 흐름\n5  상세·운용 범위\n0  돌아가기")
            choice = self.read("선택 [0] > ",
                               "1번은 예시 관측입니다. 2번 작업은 별도 승인합니다. 돌아가기/종료는 중지 요청이 아닙니다.")
            if choice in {"", "0", ":back"}:
                return
            if choice == "1":
                self.observe(node)
            elif choice == "2":
                if action:
                    self.mutate(node, action)
                else:
                    self.say(label + ". 먼저 표시된 안내를 확인하세요.")
            elif choice == "3":
                self.screen("최근 정제 기록 [합성 예시]")
                self.say(f"node={node.node_id} / attempt=demo-attempt-{node.attempt}")
                self.say("마지막 예시 기록: " + node.last)
                self.say("원문 stdout/stderr·키·비밀번호는 표시하지 않습니다. 실시간 로그 조회가 아닙니다.")
                self.pause()
            elif choice == "4":
                self.report(node)
            elif choice == "5":
                self.screen("상세 · 실제 상태를 조회하지 않는 설명 화면")
                self.say(f"노드 HTTP: 127.0.0.1:{node.http_port} / P2P: 192.0.2.10:{node.p2p_port}")
                self.say("범위: Mac 현재 사용자 LaunchAgent / 수동 시작 / 자동 재시작 없음")
                self.say("터미널 종료와 노드 중지는 별도입니다. Mac 로그아웃 시 노드도 종료됩니다.")
                self.say("CLI 불일치 때 제품은 검증된 설치 CLI의 사용 방법을 안내합니다.")
                self.say("이 데모는 CLI/identity/attempt/잠금 검증을 실제 수행하지 않습니다.")
                self.pause()
            else:
                self.say("표시된 번호를 선택하세요.")

    def observe(self, node):
        if not node.observed:
            self.say("[예시 관측] 저장된 기록과 별도로 현재 상태를 확인한 상황입니다.")
        elif node.state in {"starting", "timeout"}:
            node.state = "ready"
            self.say("[예시 관측] 같은 실행 시도가 이제 READY입니다. 새 시작 요청은 보내지 않았습니다.")
        elif node.state == "stopping":
            node.state = "stopped"
            self.say("[예시 관측] 정상 stop report·프로세스 부재·잠금·등록 정리가 모두 확인된 상황입니다.")
        node.observed = True
        node.last = STATES[node.state] + " [이번 데모 관측]"
        if node.state == "unknown":
            self.say("같은 상태 조회만으로 결과 불명을 해결하지 못합니다. 지원 보고서를 확인하세요.")

    def mutate(self, node, action):
        descriptions = {
            "init": "새 DB를 초기화합니다. chain/genesis·자산 등록 정책을 확정하며 시작은 하지 않습니다.",
            "start": "Mac 현재 사용자 LaunchAgent로 시작합니다. 터미널을 닫아도 유지되나 로그아웃 시 종료됩니다.\n다음 로그인 때 자동으로 시작하지 않습니다.",
            "stop": "선택한 실행 시도에 정상 종료를 요청합니다. 데이터·키·설치본·기록을 보존합니다.\n정상 종료 확인 뒤에도 재시작은 별도 승인합니다.",
        }
        if not self.confirm(f"{node.name} / demo-attempt-{node.attempt}\n" + descriptions[action]):
            return
        if node.scenario == "changed":
            node.attempt += 1
            node.scenario = "normal"
            node.observed = False
            self.say("[예시] 확인 중 다른 CLI가 실행 대상을 변경했습니다. 승인을 폐기했습니다.")
            self.say("이번 승인을 새 대상에 적용하지 않았습니다. 상태를 다시 확인하세요.")
            return
        if action == "init":
            node.state = "initialized"
            self.say("[예시 결과] 초기화 완료. 아직 시작하지 않았습니다.")
        elif action == "start":
            node.state = "starting"
            node.attempt += 1
            self.say("[예시 결과] 시작 요청 수락. 1번으로 준비 상태를 확인하세요.")
        elif action == "stop":
            node.state = "stopping"
            self.say("[예시 결과] 중지 요청 수락. 1번으로 정상 종료 여부를 확인하세요.")
        node.observed = True
        node.last = STATES[node.state] + " [이번 데모 결과]"

    def report(self, node):
        self.screen("지원 보고서 · 화면 미리보기")
        self.say("범위: 최신 초기화·서비스 시도의 정제한 저장 기록")
        self.say("현재 건강 상태를 판정하거나 원문 로그·자격 자료를 자동 추가하지 않습니다.")
        self.say("제품에서는 새 파일로 저장하고 위치·누락 자료를 안내합니다. 자동 전송하지 않습니다.")
        if self.confirm("지원 보고서의 예시 내용을 화면에서 확인합니다. 파일을 저장하지 않습니다."):
            self.say("\n[합성 보고서 미리보기 · 저장되지 않음]")
            self.say(f"노드: {node.node_id}\n시도: demo-attempt-{node.attempt}")
            self.say("저장 기록: " + node.last)
            if node.state == "unknown":
                self.say("partial=true · 예시 사유: 종료 report 누락. 지원 검토 필요.")
            else:
                self.say("partial=false [예시] · 보고서 생성과 노드 건강 상태는 별도입니다.")
            self.pause()

    def load_scenario(self, key):
        title, state = SCENARIOS[key]
        self.say("새 예시 상태를 불러옵니다: " + title)
        node = Node("문제 상황 · " + title, state, scenario=key,
                    last=STATES[state] + " [이전 기록 예시]")
        self.nodes.append(node)
        self.operate(node)

    def scenario_menu(self):
        keys = list(SCENARIOS)
        self.screen("상황 선택 [데모 전용 · 제품 메뉴가 아닙니다]")
        for i, key in enumerate(keys, 1):
            self.say(f"{i}  {SCENARIOS[key][0]}")
        self.say("0  홈으로")
        while True:
            choice = self.read("선택 [0] > ", "별도 예시 노드를 추가합니다. 이미 만든 체험 노드는 변경하지 않습니다.")
            if choice in {"", "0", ":back"}:
                return
            if choice.isascii() and choice.isdigit() and 1 <= int(choice) <= len(keys):
                self.load_scenario(keys[int(choice) - 1])
                return
            self.say("목록의 번호를 선택하세요.")


def main():
    parser = argparse.ArgumentParser(description="BXDL 터미널 UX 체험. 모든 상태는 메모리 예시이며 실제 작업을 하지 않습니다.")
    parser.add_argument("--scenario", choices=SCENARIOS, help="해당 예시 노드 화면에서 시작")
    parser.add_argument("--scripted", action="store_true", help="데모 검증용 비TTY 입력 허용 (제품 CLI 옵션 아님)")
    args = parser.parse_args()
    if not args.scripted and not (sys.stdin.isatty() and sys.stdout.isatty()):
        print("대화형 터미널에서 실행하세요. 자동 검증은 데모 전용 --scripted를 사용하세요.", file=sys.stderr)
        return 2
    try:
        Demo().run(args.scenario)
    except ExitDemo:
        pass
    print("\n체험을 종료했습니다. 이번 실행의 예시 상태는 사라집니다.")
    print("실제 설치·노드·파일에는 아무 작업도 하지 않았습니다.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
