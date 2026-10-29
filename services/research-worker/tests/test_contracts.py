from cers_contracts import GraphProposal, ProposedNode


def _proposal() -> GraphProposal:
    return GraphProposal(
        id="prop_01j9m2",
        project_id="prj_01j9m2",
        anchor_node_id="node_01j9m2",
        base_graph_revision=3,
        proposed_nodes=[
            ProposedNode(
                client_id="c1",
                type="research",
                title="候选研究",
                goal="验证一个独立问题",
            )
        ],
        proposed_edges=[],
        status="draft",
        created_at="2026-10-28T00:00:00Z",
        updated_at="2026-10-28T00:00:00Z",
    )


def test_wire_format_uses_camel_case() -> None:
    data = _proposal().model_dump(by_alias=True)
    assert data["projectId"] == "prj_01j9m2"
    assert data["baseGraphRevision"] == 3
    assert data["proposedNodes"][0]["clientId"] == "c1"
    assert "project_id" not in data
