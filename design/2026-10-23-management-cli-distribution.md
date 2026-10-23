# 독립 Mac 관리 CLI 배포·업데이트 — CLI05-02

- 상태: **DONE — 구현·자동 검사·실제 Mac signed13/unsigned13 인수 PASS**
- 근거: [최종 검증 기록](../results/2026-10-23-cli05-management-distribution.md) · 합성 revision의 개발 인수이며 공식 release는 별도
- 후속: [CLI05-03 지정 Mac 서비스·네트워크 인수](../results/2026-10-23-cli05-runtime-acceptance.md) PASS. 실제 Git commit의 CLI 배포본·고정 RC.3 조합이며 진단 완전수집 OPEN, CLI05-04 사람 과업 NOT_RUN
- 기준: [관리 CLI/노드 실행기 분리](./2026-10-23-independent-management-cli.md), [CLI 우선 구현 계획](./2026-10-22-integrated-operations-ux-implementation-plan.md)
- 선행: CLI05-01 zsh 자동완성 완료, 기존 `BXDL_RC3_EXECUTOR_V1` 유지
- 대상: macOS arm64 사용자 권한 설치. NIGO·JRE·노드 package 재배포는 포함하지 않는다.

## 사용자 흐름

사용자는 미리 준비한 CLI 전용 배포본으로 설치하고, 이후 새 로컬 배포본을 명시해 업데이트한다. 네트워크에서 최신 버전을 찾거나 내려받지 않는다. 기존 노드 패키지용 `bxdl install`과 구분해 `bxdl cli` 명령을 둔다.

```sh
./bin/bxdl cli install --public-key /trusted/bxdl-cli-public.pem
bxdl cli info
bxdl cli update --from <새-배포본>
bxdl cli rollback
```

명시 설치·업데이트·복귀 명령이 작업 요청이며 추가 번호 메뉴나 TTY 문답은 없다. 자동화는 기존 JSON envelope를 사용하고 stdin을 소비하지 않는다. 명령별 입력·신뢰 정책은 [사용 가이드](../docs/management-cli.md)를 따른다. 자동 검사와 실제 Mac 결과는 위 검증 기록에서 구분하며 CLI05-03/04의 완료로 확대하지 않는다.

## 저장과 실행 경로

```text
~/Library/Application Support/BXDL-CLI/
  .bxdl-cli.json
  .lock
  releases/<version>-<revision>/
    admission.json
    payload/
      bin/bxdl
      share/zsh/site-functions/_bxdl
      docs/management-cli.md
      docs/completion.md
      manifest.json
      manifest.sig                 # 서명 배포본에만 존재
  staging/
  activations/<순번>/
    selection.json
    bin -> ../../releases/<선택 버전>/payload/bin
    share -> ../../releases/<선택 버전>/payload/share
  current -> activations/<선택 순번>

~/.local/bin/bxdl -> <CLI root>/current/bin/bxdl
```

기본 관리 home인 `~/Library/Application Support/BXDL`에는 catalog·sessions·operations 등이 있고 허용된 하위 이름을 검사한다. CLI 배포 root는 sibling `BXDL-CLI`로 분리해 그 형식과 이름 목록을 건드리지 않는다. `--root`와 `--bin-dir`은 CLI 설치 위치를 명시하는 옵션이며 노드 목록의 `--home`과 별개다.

기존 실행 진입점이 BXDL이 소유한 링크가 아니면 덮어쓰지 않는다. root·entry·release 디렉터리의 소유, 링크 및 파일 조건을 확인하고 고객 control/data나 임의 자료를 설치 root로 채택하지 않는다. 같은 사용자 프로세스 사이의 악의적 변경을 격리하는 보안 경계로 주장하지 않는다.

## 배포 계약과 식별

CLI 전용 archive는 바이너리·zsh 자동완성·두 사용 가이드와 manifest를 담는다. 기존 노드용 manifest의 engine/runtime 필수 필드를 가짜 값으로 채우지 않는다. CLI 전용 kind/schema를 사용하고 고정 payload의 경로·크기·mode·SHA-256과 archive 한도를 검사한다. 디렉터리 형태의 배포본도 같은 manifest와 파일 검사를 거친다.

관리 CLI version과 실제 Git revision, `darwin-arm64` target을 고정한다. 배포 빌드는 revision을 바이너리에 주입하며 배포 identity와 실제 `version --json` 응답을 대조한다. 같은 version/revision에 다른 bytes를 새 버전으로 채택하지 않는다. 개발 fixture의 합성 revision 또는 unsigned opt-in은 정식 배포 identity와 구분해 기록한다.

서명 검증은 archive 밖의 명시 신뢰키를 기준으로 한다. 첫 서명 설치에서 공개키를 고정하고 이후 서명 업데이트는 저장한 pin을 재사용한다. 서명 없는 개발 자료는 설치·업데이트마다 명시 opt-in이 필요하다. 서명 성공이나 checksum 일치를 Apple code signing·notarization·정식 고객 배포 승인으로 표현하지 않는다. 키 교체와 다른 신뢰 정책으로의 자동 전환은 허용하지 않는다.

## 설치·업데이트·복귀의 순서

1. 새 공급물의 형식·inventory·신뢰 조건·지원 플랫폼을 확인한다.
2. 초기 root는 소유·신뢰 marker를 포함한 sibling 임시 디렉터리에서 완성한 뒤 게시한다. 각 버전은 staging에서 payload와 검증 admission을 같은 wrapper에 완성하고 파일을 다시 대조한다. 검증 전 후보 바이너리를 실행하지 않는다.
3. 검증한 바이너리의 제한된 `version --json` 응답을 대조한다. engine·service 명령은 실행하지 않는다. 검증된 wrapper를 기존 경로를 교체하지 않는 rename으로 releases에 한 번 게시해 payload와 admission 사이의 중단 불일치를 피한다.
4. 현재 선택과 설치 기록을 재검사한 뒤 `current`를 한 번 원자적으로 전환한다. 현재/이전 버전 선택 기록과 bin/share 링크는 같은 immutable activation 안에 둬 선택 링크와 별도 상태 파일 사이의 불일치를 피한다. 완료 전 실패는 기존 선택을 유지하며 부분 출력과 확정된 설치를 구분한다.
5. 이전 버전을 보존하고 명시 `rollback`으로 복귀한다. 동일 배포본 재요청은 검증 후 불필요한 재설치·전환을 하지 않는다.

동시 업데이트, 다른 파일로 바뀐 진입점, 같은 identity의 다른 bytes, 손상 release, 권한 실패 및 전환 중단을 회귀 검사한다. `info`는 실제 호출 CLI와 설치된 선택을 구분해 알려 주며 누락된 root를 자동 생성하지 않는다. 복구할 수 없는 불일치에서 임의 정리나 덮어쓰기 대신 확인 가능한 오류를 남긴다.

## 셸 연결

CLI와 자동완성은 같은 release에서 함께 전환한다. PATH·fpath의 고정 경로를 안내하고 `.zshrc`를 자동으로 편집하지 않는다. 설치 뒤 어떤 `bxdl`을 선택하는지 확인할 수 있게 하며 셸 hash·alias·다른 설치본의 우선순위를 사용자 안내에 포함한다. 업데이트 후 현재 zsh가 이미 읽은 함수는 `source <(bxdl completion zsh)`로 다시 읽고 새 터미널에서도 선택한 CLI와 완성을 확인한다.

## 구현과 인수 단위

| 단위 | 구현 | 완료 근거 |
| --- | --- | --- |
| 배포 계약 | CLI-only manifest·bounded build/verify·revision 주입 | 경로/형식/한도·서명·hash·dirty build·identity 회귀 |
| 설치 저장소 | 별도 root·release·소유 entry·현재 선택·이전 버전 | 중단/손상/충돌/동시 전환에서 기존 선택과 자료 보존 |
| 명령과 셸 UX | install/update/info/rollback·help·completion·사용 안내 | 비TTY/JSON/NoInput, 현재 경로/버전, fresh zsh Tab |
| 실제 Mac 인수 | 서로 다른 두 CLI 배포물로 설치→업데이트→복귀 | 기존 catalog/sessions/control/data 불변과 선택한 바이너리/완성 대조 |

자동 검사의 fixture 실행과 실제 release 바이너리/ZLE 인수를 구분해 기록한다. 이 CLI05-02의 당시 결과는 실제 노드·LaunchAgent·네트워크 시험을 포함하지 않았다. 이후 [CLI05-03](../results/2026-10-23-cli05-runtime-acceptance.md)에서 설치 진입점을 사용한 서비스16사례와 실행 중 CLI 전환3회·4-validator·두 복구·wrong-pin·정상 정리를 확인했다. 동일 source tree의 실제 PR head/merge commit을 각각 빌드했으며 기존 RC.3 payload/JRE/worker는 그대로다. 단일 노드 진단은 실제 partial의 정확한 표현을 인수한 범위로, 완전수집은 OPEN이다. 작성자가 아닌 사용자의 CLI05-04 과업은 NOT_RUN으로 남긴다. Linux 실행, Mac Intel, `.pkg`/Homebrew, 자동 온라인 업데이트, 노드 upgrade/migration·제거와 정식 공급망 인수는 이번 단계의 완료 조건에 포함하지 않는다.
