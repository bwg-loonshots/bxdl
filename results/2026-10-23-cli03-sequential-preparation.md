# CLI-03 순차 설치 준비·조회·수정 검증

- 기준: `adea89b653ee77e31b2b13f92493414c6f03a564`에서 시작한 `codex/cli03-sequential-setup` 작업 트리.
- 구현: 직접 `setup new/resume`의 순차 문답, `setup list/show/edit`. 기존 Preparation 저장 형식과 console의 번호 메뉴는 보존한다.
- 결과: **PASS**. [정제 JSON 결과](./2026-10-23-cli03-sequential-preparation.json), [사용 가이드](../docs/preparation.md), [구현 계획](../design/2026-10-22-integrated-operations-ux-implementation-plan.md).

## 검증 범위

직접 CLI가 공급 자료 → 자기 노드 → 필요한 제품/정책 → 검토 순서로 진행한다. 저장한 답은 재사용하고, 중단 후 부족한 질문부터 재개한다. `:help`, 이전 질문, 취소/EOF, 검토의 `edit <항목>`과 명시적인 설정 준비 승인을 확인했다. 준비 성공 후 종료하며 동일한 유효 결과를 다시 승인하면 새 revision을 만들지 않는다.

`setup list/show`는 비TTY·JSON 조회를 지원한다. 없는 관리 루트를 만들지 않고 기존 배타 잠금을 획득하거나 해제하지 않는다. 파일 내용·권한·inode·이름 목록의 무변경을 확인했다. 접근 시각은 무쓰기 비교 대상에서 제외한다. 손상된 이름 있는 항목은 부분 결과로 표시하고, 저장된 `PREPARED_RECORDED`와 현재 자료 `VERIFIED/CHANGED`를 분리한다. 공개 네트워크와 저장 노드가 다르면 다음 입력을 다시 안내한다.

`setup edit`은 지정한 제품 필드만 저장한다. 동일 값·취소·EOF는 기록을 보존하고 실제 변경은 이전 검토만 무효화한다. 기존 설정 출력의 내용과 inode는 보존한다. 입력 대기 중 동일 바이트의 checkpoint를 다른 inode로 바꿔도 정상 완료로 채택하지 않는다. credential 원문과 참조 경로는 공개 조회/문답 결과에 포함하지 않는다.

## 실행한 검사

| 검사 | 결과 | 근거/범위 |
| --- | --- | --- |
| `make check` | PASS | fmt·Clippy, Rust 525개, Python 50개 |
| 최종 `prepare_cli` | PASS | 26개. 전체 검사에 포함된 25개에 다음 입력 의존 순서 1개를 추가해 재실행 |
| 최종 fmt·Clippy | PASS | 위 추가 테스트까지 포함 |
| `setup_commands` | PASS | 8개, 전체 검사에도 포함 |
| `make check-linux` | PASS | x86_64 Linux 타입 검사. Linux 실행/서비스 시험 아님 |
| 실제 Mac 터미널·비TTY | PASS | PTY 8개 + JSON 조회 3개, 아래 사례 |
| `make build` | PASS | 최신 관리 CLI를 `bin/bxdl`에 빌드 |
| release CLI smoke | PASS | 없는 별도 home의 JSON 빈 목록 조회, 관리 폴더 미생성 |

전체 검사 뒤 추가한 1개를 합한 고유 Rust 테스트는 526개다. 추가 테스트는 실행 중인 전체 검사 바이너리에 뒤늦게 포함됐다고 간주하지 않는다.

첫 전체 검사에서는 번호 메뉴용 입력 스크립트가 남아 있던 기존 단위 테스트 1개가 실패했다. 새 순차 입력으로 갱신하고 출력 문구 대신 저장된 준비 상태/CLI 대응 상태도 확인하도록 수정한 뒤 전체 검사를 다시 통과했다. 첫 PTY 하네스 시도는 종료 직후 EIO와 프로세스 회수 순서를 잘못 처리해 중단됐으며, 하네스를 수정한 최종 실행에서 아래 11개를 통과했다.

## 실제 Mac 사례

1. 새 준비 생성 승인 뒤 취소: exit 5, 재개 가능한 기록만 저장.
2. 재개하여 누락된 archive 입력 후 중단: 입력 저장.
3. 다시 재개하면 archive를 건너뛰고 신뢰 정책 질문부터 시작. EOF 뒤 기존 기록 보존.
4. 미완성 초안의 제품 한 항목 수정 후 종료.
5. 비TTY `setup list --json`: 읽기 전용.
6. 비TTY `setup show --json`: 읽기 전용.
7. 가져온 제품으로 전체 순차 준비를 승인해 설정 파일만 생성.
8. `setup show --json`: 현재 자료 `VERIFIED` 확인.
9. 동일 준비 재승인: 기존 기록/파일을 변경하지 않음.
10. 한 항목 수정: 검토 무효화, 기존 생성 파일 보존.
11. 재개 후 새 revision 준비: 이전 파일을 덮어쓰지 않음.

Mac arm64의 실제 터미널 입출력을 사용했다. 패키지·JAR·Java는 실행되지 않는 합성 fixture이며 운영용 키를 사용하지 않았다. 공급 원자료는 불변이고 패키지 설치·control·data 운영 루트는 생성되지 않았다. 원문 PTY, 시험 자격 자료와 바이너리는 저장소에 포함하지 않는다.

## 빌드 식별과 한계

- 실제 PTY debug CLI SHA-256: `ad6150d0b0b1b51e5c7e22f36c28e5bfe4b119cb927f3f7d971d7a56cbbb4e38`.
- release CLI SHA-256: `96473572c1460c7b1351c6f3703d328e7144783c39854d4259b37bced579b7a6`.
- PTY 소스 목록 86개 SHA-256: `46be9a756c91e1fae1313df43c25e1707edefb2690ca089b53f741ea4bb8ed04`. 최종 소스와 재대조해 변경이 없음을 확인했다.
- 전체 검사 로그 SHA-256: `4361b4c7d32eb96371976f34e0c653e415bb0f1672c3e19a0f02516e11cd3e19`.

이번 근거는 설치 준비 UX와 저장·조회 계약이다. NIGO/RC.3/JRE pin과 패키지는 변경하지 않았으며 엔진·서비스·합의·거래를 재인수한 결과가 아니다. `setup apply`와 이름 기반 초기화는 CLI-04, 자동완성과 실제 사용자 과업/운용 조합 인수는 CLI-05에 남긴다.
