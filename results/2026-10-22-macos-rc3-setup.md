# RC.3 동일 패키지 설치·원본 공급 경로 부재 검증 — 2026-10-22

**PASS — 같은 RC.3 서명 archive의 지정 설치·로컬 운용 13사례.** [4-validator 재인수](./2026-10-22-macos-rc3-package.md)가 정상 종료한 뒤 그 archive와 공개 검증 key를 새 원본 공급 경로로 복사했다. 별도 새 설치·control·data·setup 세션에서 등록하고 원본 archive·key·lock 디렉터리를 옮겨 세 원본 경로가 존재하지 않는 상태로 후속 운용을 검증했다. 원본 bytes는 보존했으며 package를 다시 조립하거나 서명하지 않았다.

[정제 JSON](./2026-10-22-macos-rc3-setup.json), [설계](../design/2026-10-22-rc3-package-reacceptance.md)를 따른다. 기존 RC.2 A/B의 INCOMPLETE와 특정 파일/TCC 원인 미확정은 그대로 유지한다.

## 실행 조합

| 항목 | 값 |
| --- | --- |
| BXDL 제품 source | `53bb11b90d9d51aba5a7b509074046fd6ee78401`와 동일 |
| CLI SHA-256 | `b7d677b44de30275315ef4a7ccf1e4230b66034d1964c27cbbd3bc586c622cef` |
| NIGO RC.3 source | `7ec5abd8c98e1cf070763a16f815e9281b74a5c1` |
| JAR SHA-256 | `05a38bd1de45d16f0529a8689c113d6a359989ff0cd95e4c507bfc49843ee46f` |
| JRE | 기존 선정 Temurin21.0.12.1+1-LTS full JRE, macOS arm64 |
| archive SHA-256 | `8a45cb8bf3482333ad43e8c29ad24c2adde8d6fe778eaf27fe2f5c2c842eb017` |
| manifest SHA-256 | `2647e9bfb65be83a532a4f37523dcc9ab54492e609d10a646cd0bf535109d123` |
| payload | 270파일 / 325,299,323 bytes |

제품 Rust source와 CLI는 머지 revision에 대응한다. 새 후보 선택 자료와 인수 하네스는 실행 당시 미커밋 상태이며 별도 hash로 고정했다. 자격 자료는 같은 RC.3 full 시험 fixture의 복사본이며 이 단계에서 새 키를 발급했다고 주장하지 않는다. full 시험의 모든 서비스를 정상 종료한 뒤 실행해 동일 identity의 중복 실행을 피했다. consensus auto-start·sync를 끄고 peer 목록을 비운 단일 노드 로컬 profile이며 이 실행의 `globalConsensus`는 NOT_CHECKED다.

## 확인한 흐름

1. full 시험과 archive/manifest/CLI hash가 같은 외부 Ed25519 서명 package를 확인했다.
2. 저장한 제품 설정을 `--from`으로 가져오고 실제 PTY의 공급 자료 문답·계획·apply 승인으로 새 설치와 V2 등록을 완료했다. `trust/` 세 파일의 SHA 일치와0700/0600 권한을 확인했다.
3. 원본 공급 세 파일의 경로가 모두 없는 상태에서 `setup --resume`와 `check`를 통과했다. cold 검사는 MATCHED이며 계약상 INCOMPLETE를 유지했다.
4. 설치 도우미에서 명시 init·start를 각각 승인하고 새 DB 초기화와 로컬 READY를 확인했다. status/diagnose에 ENGINE_EXEC가 표시됐다.
5. 정상 stop 후 같은 DB를 새 attempt로 재시작해 READY를 확인하고 다시 정상 종료했다. 자동 재시작·재초기화·강제 종료는 사용하지 않았다.
6. 원본 보존본·관리 보관본·운용 입력·제품 source와 CLI의 불변을 확인했다.

하네스의 지정 사례13개가 PASS다. 독립 검사에서도 설치 payload270파일의 정확한 membership·hash·size·mode, 두 attempt 각각의 7단계 진입 순서, 기록된14 PID 잔존0, 두 LaunchAgent NOT_LOADED, 시험2포트 TCP LISTEN0을 확인했다. bind 가능 여부를 종료 판정으로 사용하지 않았다. 단계 자체를 완료 증거로 삼지 않고 READY·정상 STOPPED를 별도로 확인했다.

실행 raw26파일과 이를 포함한 보존본29파일의 별도 inventory를 고정했다. 원 결과·독립 검사·하네스·source·inventory hash는 정제 JSON에 연결했다. private key·자격 자료·archive·DB·미정제 출력은 Git에 넣지 않는다. 저장소 검사는 Rust363개·Python50개·fmt·clippy·Mac release build PASS이며 이 실행의 실제 lifecycle 근거와 구분한다.

## 범위와 남는 작업

이 시험은 완성된 제품 설정을 가져오는 설치 흐름이다. **14개 제품 설정 문답 전체·수정·취소/EOF·터미널 hangup 행렬을 RC.3에서 다시 실행한 결과가 아니다.** 해당 RC.2 이력은 별도로 보존한다. 로그인/로그아웃·sleep·전원 차단·mutation 중단, 운용 자료 전체 경로의 접근 권한, 기존 설치 upgrade·binding 변환·안전 제거와 전체 G1-M도 남아 있다.

새 공급 보관본 동작이 과거 Documents 경로 대기의 원인을 확정하거나 모든 TCC 문제를 해결했다는 의미는 아니다. V1은 자동 변환하지 않으며 V2의 제품/native 설정·chain·키 참조·설치/control/data는 계속 필요하다. RC.3 offline Storage A7은 이번 lifecycle/4-validator 인수와 별도이고 NOT_RUN이다. 정식 고객 배포·J5 공급망 승인을 선언하지 않는다.
